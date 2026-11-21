FROM python:3.12-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates jq \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --create-home --shell /bin/bash agent \
    && mkdir -p /etc/magpie

COPY magpie_agent.py /usr/local/bin/magpie-agent
COPY entrypoint.sh /usr/local/bin/agent-entrypoint.sh
RUN chmod +x /usr/local/bin/magpie-agent /usr/local/bin/agent-entrypoint.sh

USER 10001:10001
WORKDIR /home/agent
ENTRYPOINT ["/usr/local/bin/agent-entrypoint.sh"]
