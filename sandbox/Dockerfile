FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y \
    python3 python3-pip python3-venv \
    nodejs npm \
    curl wget jq git \
    && rm -rf /var/lib/apt/lists/*
RUN pip3 install --break-system-packages playwright httpx beautifulsoup4 pandas \
    && playwright install --with-deps chromium
RUN mkdir -p /workspace /memory /scratchpad
WORKDIR /workspace
CMD ["sleep", "infinity"]
