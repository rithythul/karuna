.PHONY: setup dev build test clean infra sandbox check

# Start PostgreSQL and Redis
infra:
	docker compose up -d

# Build sandbox Docker image
sandbox:
	cd sandbox && docker build -t karuna-sandbox:latest .

# Full setup: infra + sandbox + frontend deps
setup: infra sandbox
	cd frontend && bun install

# Dev: start infra + backend + frontend
dev: infra
	@echo "Starting Karuna..."
	@echo "Backend:  http://localhost:8000"
	@echo "Frontend: http://localhost:3000"
	@SQLX_OFFLINE=true cargo run &
	@cd frontend && bun run dev

# Build everything for production
build:
	SQLX_OFFLINE=true cargo build --release
	cd frontend && bun run build

# Check compilation without building
check:
	SQLX_OFFLINE=true cargo check

# Run tests
test:
	SQLX_OFFLINE=true cargo test

# Clean everything
clean:
	docker compose down -v
	cargo clean
	rm -rf frontend/.next frontend/node_modules
