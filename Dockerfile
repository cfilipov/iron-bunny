# Stage 1: Build SvelteKit frontend
FROM node:20-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# Stage 2: Build Rust binary (with embedded frontend)
FROM rust:1.88-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
COPY commands.yml ./
COPY migrations/ ./migrations/
COPY templates/ ./templates/
COPY static/ ./static/
COPY --from=frontend /app/frontend/dist ./frontend/dist
RUN cargo build --release

# Stage 3: Minimal runtime image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/brunnylol /usr/local/bin/iron-bunny
COPY --from=builder /app/commands.yml /app/commands.yml
COPY --from=builder /app/templates/ /app/templates/
COPY --from=builder /app/static/ /app/static/
COPY --from=builder /app/migrations/ /app/migrations/
WORKDIR /app
RUN mkdir -p /data
VOLUME /data
ENV BRUNNYLOL_PORT=8000
ENV BRUNNYLOL_DB=/data/brunnylol.db
EXPOSE 8000
CMD ["/usr/local/bin/iron-bunny"]
