FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive

# System packages
RUN apt-get update && apt-get install -y \
    python3 python3-pip python3-venv \
    curl wget jq git zip unzip \
    tree file htop \
    sqlite3 \
    imagemagick \
    poppler-utils \
    libreoffice-calc-nogui \
    ca-certificates gnupg \
    && rm -rf /var/lib/apt/lists/*

# Node.js 20 via nodesource
RUN mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
       | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
    && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_20.x nodistro main" \
       > /etc/apt/sources.list.d/nodesource.list \
    && apt-get update && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# agent-browser CLI for browser automation
RUN npm install -g @anthropic-ai/agent-browser \
    && npx playwright install --with-deps chromium

# Python packages: data science, web scraping, utilities
RUN pip3 install --no-cache-dir \
    httpx beautifulsoup4 \
    pandas numpy scipy \
    matplotlib seaborn plotly \
    scikit-learn \
    openpyxl xlsxwriter \
    Pillow \
    pyyaml toml \
    requests lxml \
    tabulate rich \
    fastapi uvicorn flask \
    pytest

# Create workspace directories
RUN mkdir -p /workspace /memory /scratchpad /workspace/artifacts /workspace/deploy

WORKDIR /workspace
CMD ["sleep", "infinity"]
