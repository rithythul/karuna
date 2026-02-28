FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive

# System packages
RUN apt-get update && apt-get install -y \
    python3 python3-pip python3-venv \
    nodejs npm \
    curl wget jq git zip unzip \
    tree file htop \
    sqlite3 \
    imagemagick \
    poppler-utils \
    libreoffice-calc-nogui \
    && rm -rf /var/lib/apt/lists/*

# Python packages: browser automation, data science, web scraping, utilities
RUN pip3 install --no-cache-dir \
    playwright httpx beautifulsoup4 \
    pandas numpy scipy \
    matplotlib seaborn plotly \
    scikit-learn \
    openpyxl xlsxwriter \
    Pillow \
    pyyaml toml \
    requests lxml \
    tabulate rich \
    fastapi uvicorn flask \
    pytest \
    && playwright install --with-deps chromium

# Create workspace directories
RUN mkdir -p /workspace /memory /scratchpad /workspace/artifacts /workspace/deploy

WORKDIR /workspace
CMD ["sleep", "infinity"]
