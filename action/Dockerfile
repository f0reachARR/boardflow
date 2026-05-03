FROM kicad/kicad:9.0

USER root

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-pip \
    xvfb \
    jq \
    curl \
    zip \
    ca-certificates \
    python3-yaml \
    && rm -rf /var/lib/apt/lists/*

RUN pip3 install --break-system-packages interactivehtmlbom

COPY entrypoint.sh /action/entrypoint.sh
COPY lib/ /action/lib/

RUN chmod +x /action/entrypoint.sh /action/lib/*.sh

ENTRYPOINT ["/action/entrypoint.sh"]
