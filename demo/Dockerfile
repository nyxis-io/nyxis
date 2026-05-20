# Optional Python app service (legacy compose). Browser traffic is served by nginx
# with static mounts for demo/, bench/, and nyxis-drivers/js → /sdk.
#
# Build from nyxis/:
#   docker compose build
#   docker compose up
#
# Open http://localhost:8000/demo/ and http://localhost:8000/bench/bench.html

FROM python:3.12-slim-bookworm

WORKDIR /app
COPY demo/server.py .

EXPOSE 8000
CMD ["python3", "server.py", "8000"]
