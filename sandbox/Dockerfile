FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y \
    python3 python3-pip python3-venv \
    curl wget jq git unzip \
    && rm -rf /var/lib/apt/lists/*
ENV BUN_INSTALL=/usr/local
RUN curl -fsSL https://bun.sh/install | bash -s "bun-v1.3.10"
RUN pip3 install playwright httpx beautifulsoup4 pandas \
    && playwright install --with-deps chromium
RUN mkdir -p /workspace /memory /scratchpad
WORKDIR /workspace
CMD ["sleep", "infinity"]
