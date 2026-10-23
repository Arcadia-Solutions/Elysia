# Telemetry

Logs always go to stdout, at `info` level unless `RUST_LOG` says otherwise.

With `telemetry.otlp_endpoint` set in `config.yml`, the backend also exports
traces, metrics and logs over OTLP gRPC to any OpenTelemetry collector.

## Local Grafana stack

The `telemetry` compose profile runs Grafana's all-in-one OpenTelemetry image
plus a collector for the host's CPU, memory, disk and network metrics:

```sh
docker compose --profile telemetry up -d
```

1. Set `telemetry.otlp_endpoint: http://localhost:4317` and restart the backend.
2. Open Grafana at <http://localhost:3001> and log in as `admin` with the
   `GF_SECURITY_ADMIN_PASSWORD` environment variable (`elysia` by default).
3. Import `opentelemetry/dashboard.json` from Dashboards > New > Import.
