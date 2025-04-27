FROM debian:bullseye-slim

# Define the Docker Compose version and SHA256 hash for amd64/x86_64
ARG COMPOSE_VERSION=2.34.0
ARG DOCKER_COMPOSE_SHA256_AMD64=94a416c6f2836a0a1ba5eb3feb00f2e700a9d98311f062c4c61494ccbf3cd457

# Install prerequisites, download Docker Compose for linux-x86_64, verify checksum, set permissions, and clean up in a single layer
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        openssh-client \
        curl \
        ca-certificates; \
    TARGET_ARCH="linux-x86_64"; \
    EXPECTED_SHA256="$DOCKER_COMPOSE_SHA256_AMD64"; \
    COMPOSE_URL="https://github.com/docker/compose/releases/download/v${COMPOSE_VERSION}/docker-compose-${TARGET_ARCH}"; \
    curl -SL "$COMPOSE_URL" -o /usr/local/bin/docker-compose; \
    echo "${EXPECTED_SHA256}  /usr/local/bin/docker-compose" | sha256sum -c -; \
    chmod +x /usr/local/bin/docker-compose; \
    rm -rf /var/lib/apt/lists/*; \
    docker-compose --version

# Add dcd binary (copied from build context)
COPY dcd /usr/local/bin/dcd
# Set execute permissions for dcd
RUN set -eux; \
    chmod +x /usr/local/bin/dcd; \
    dcd --version

# Add entrypoint script
COPY entrypoint.sh /entrypoint.sh
# Set execute permissions for entrypoint script
RUN chmod +x /entrypoint.sh

# Set the entrypoint
ENTRYPOINT ["/entrypoint.sh"]
