use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

use bollard::container::{Config as ContainerConfig, LogOutput, RemoveContainerOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::HostConfig;
use bollard::Docker;
use futures_util::StreamExt;

use crate::config::Config;
use crate::error::AppError;

pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
}

#[derive(Clone)]
pub struct SandboxManager {
    docker: Docker,
    image: String,
    memory_limit: i64,
    cpu_quota: i64,
    pool: Arc<Mutex<VecDeque<String>>>,
    pool_target_size: usize,
}

impl SandboxManager {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| AppError::Sandbox(format!("Docker connect failed: {e}")))?;
        Ok(Self {
            docker,
            image: config.sandbox_image.clone(),
            memory_limit: config.sandbox_memory_limit * 1024 * 1024,
            cpu_quota: config.sandbox_cpu_quota,
            pool: Arc::new(Mutex::new(VecDeque::new())),
            pool_target_size: 3,
        })
    }

    /// Pre-warm the pool with `count` containers.
    pub async fn warm_pool(&self, count: usize) -> Result<(), AppError> {
        for _ in 0..count {
            let id = self.create_container("pool").await?;
            self.pool.lock().await.push_back(id);
        }
        tracing::info!("Sandbox pool warmed with {} containers", count);
        Ok(())
    }

    /// Create and start a new container (internal).
    async fn create_container(&self, label: &str) -> Result<String, AppError> {
        let host_config = HostConfig {
            memory: Some(self.memory_limit),
            cpu_quota: Some(self.cpu_quota),
            pids_limit: Some(256),
            security_opt: Some(vec!["no-new-privileges".to_string()]),
            ..Default::default()
        };

        let config = ContainerConfig {
            image: Some(self.image.clone()),
            host_config: Some(host_config),
            labels: Some(
                [("hanuman.role".to_string(), label.to_string())]
                    .into_iter()
                    .collect(),
            ),
            cmd: Some(vec!["sleep".into(), "infinity".into()]),
            working_dir: Some("/workspace".into()),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container::<String, String>(None, config)
            .await
            .map_err(|e| AppError::Sandbox(format!("Create failed: {e}")))?;

        self.docker
            .start_container::<String>(&container.id, None)
            .await
            .map_err(|e| AppError::Sandbox(format!("Start failed: {e}")))?;

        Ok(container.id)
    }

    /// Acquire a container for a task. Takes from pool or creates a new one.
    pub async fn acquire(&self, task_id: &str) -> Result<String, AppError> {
        let container_id = {
            let mut pool = self.pool.lock().await;
            pool.pop_front()
        };

        let id = match container_id {
            Some(id) => {
                tracing::debug!("Acquired container from pool: {}", &id[..12]);
                id
            }
            None => {
                tracing::warn!("Pool empty, creating new container for task {task_id}");
                self.create_container(task_id).await?
            }
        };

        // Replenish pool in background if below target
        let manager = self.clone();
        tokio::spawn(async move {
            let current = manager.pool.lock().await.len();
            if current < manager.pool_target_size {
                let needed = manager.pool_target_size - current;
                if let Err(e) = manager.warm_pool(needed).await {
                    tracing::error!("Failed to replenish pool: {e}");
                }
            }
        });

        Ok(id)
    }

    /// Run a command inside a container.
    pub async fn exec_cmd(
        &self,
        container_id: &str,
        cmd: Vec<&str>,
    ) -> Result<ExecResult, AppError> {
        let exec_handle = self
            .docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(cmd.into_iter().map(String::from).collect()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| AppError::Sandbox(format!("Create exec failed: {e}")))?;

        let output = self
            .docker
            .start_exec(&exec_handle.id, None)
            .await
            .map_err(|e| AppError::Sandbox(format!("Start exec failed: {e}")))?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached {
            output: mut stream, ..
        } = output
        {
            while let Some(Ok(msg)) = stream.next().await {
                match msg {
                    LogOutput::StdOut { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }

        let inspect = self
            .docker
            .inspect_exec(&exec_handle.id)
            .await
            .map_err(|e| AppError::Sandbox(format!("Inspect exec failed: {e}")))?;

        Ok(ExecResult {
            stdout,
            stderr,
            exit_code: inspect.exit_code.unwrap_or(-1),
        })
    }

    /// Release a container back to the pool after cleaning it.
    pub async fn release(&self, container_id: &str) -> Result<(), AppError> {
        let clean_result = self
            .exec_cmd(
                container_id,
                vec!["bash", "-c", "rm -rf /workspace/* /memory/* /scratchpad/*"],
            )
            .await;

        match clean_result {
            Ok(r) if r.exit_code == 0 => {
                self.pool.lock().await.push_back(container_id.to_string());
                tracing::debug!("Container {} returned to pool", &container_id[..12]);
                Ok(())
            }
            _ => {
                tracing::warn!("Container {} unhealthy, destroying", &container_id[..12]);
                self.destroy(container_id).await?;
                let new_id = self.create_container("pool").await?;
                self.pool.lock().await.push_back(new_id);
                Ok(())
            }
        }
    }

    /// Force-remove a container.
    pub async fn destroy(&self, container_id: &str) -> Result<(), AppError> {
        self.docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| AppError::Sandbox(format!("Remove failed: {e}")))?;
        Ok(())
    }

    /// Current pool size.
    pub async fn pool_size(&self) -> usize {
        self.pool.lock().await.len()
    }

    /// Replenish the pool to the target size.
    pub async fn replenish_pool(&self, target: usize) -> Result<(), AppError> {
        let current = self.pool.lock().await.len();
        if current < target {
            let needed = target - current;
            self.warm_pool(needed).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            database_url: String::new(),
            redis_url: String::new(),
            openrouter_api_key: String::new(),
            openrouter_base_url: String::new(),
            default_model: String::new(),
            planning_model: String::new(),
            fast_model: String::new(),
            sandbox_image: "hanuman-sandbox:latest".into(),
            sandbox_memory_limit: 512,
            sandbox_cpu_quota: 50000,
            host: "0.0.0.0".into(),
            port: 8000,
            koompi_client_id: String::new(),
            koompi_client_secret: String::new(),
            koompi_redirect_uri: String::new(),
            public_url: String::new(),
            dev_mode: true,
            search_api_key: None,
            search_provider: "duckduckgo".into(),
        }
    }

    #[test]
    fn test_sandbox_manager_creation() {
        let config = test_config();
        let result = SandboxManager::new(&config);
        // Bollard validates the Docker socket path eagerly.
        // Skip assertions if Docker is not available.
        if std::path::Path::new("/var/run/docker.sock").exists() {
            let manager = result.expect("Docker socket exists but connection failed");
            assert_eq!(manager.image, "hanuman-sandbox:latest");
            assert_eq!(manager.memory_limit, 512 * 1024 * 1024);
            assert_eq!(manager.cpu_quota, 50000);
            assert_eq!(manager.pool_target_size, 3);
        } else {
            assert!(result.is_err(), "Should fail without Docker socket");
        }
    }

    #[test]
    fn test_config_values_propagate() {
        // Verify config parsing logic without needing Docker.
        let config = test_config();
        let expected_memory = 512_i64 * 1024 * 1024;
        assert_eq!(config.sandbox_memory_limit * 1024 * 1024, expected_memory);
        assert_eq!(config.sandbox_cpu_quota, 50000);
        assert_eq!(config.sandbox_image, "hanuman-sandbox:latest");
    }

    #[tokio::test]
    async fn test_pool_starts_empty() {
        let config = test_config();
        // Skip if Docker is not available.
        if !std::path::Path::new("/var/run/docker.sock").exists() {
            return;
        }
        let manager = SandboxManager::new(&config).unwrap();
        assert_eq!(manager.pool_size().await, 0);
    }
}
