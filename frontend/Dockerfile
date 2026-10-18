FROM node:22-slim AS builder
WORKDIR /app
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci --no-audit
COPY . .
RUN npm run build

FROM caddy:2.11.7-alpine
COPY --from=builder /app/dist /srv/elysia
COPY Caddyfile /etc/caddy/Caddyfile
