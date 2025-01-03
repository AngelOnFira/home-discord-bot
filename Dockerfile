FROM rust:1.83-slim as builder

WORKDIR /usr/src/app
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

# Install Python and git
RUN apt-get update && apt-get install -y \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install uv using the official script and add to PATH
RUN curl -LsSf https://astral.sh/uv/install.sh | sh && \
    echo 'export PATH="/root/.local/bin:$PATH"' >> /root/.bashrc && \
    . /root/.bashrc

ENV PATH="/root/.local/bin:$PATH"

# Install python-kasa from source in a specific directory
RUN git clone https://github.com/python-kasa/python-kasa.git /opt/python-kasa && \
    cd /opt/python-kasa && \
    source $HOME/.local/bin/env && \
    uv sync --all-extras

# Copy the compiled binary from builder
COPY --from=builder /usr/src/app/target/release/home-discord-bot /usr/local/bin/home-discord-bot

CMD ["home-discord-bot"] 