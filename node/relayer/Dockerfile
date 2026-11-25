FROM node:22-alpine

WORKDIR /app
COPY node/relayer/lib.mjs node/relayer/index.mjs ./
COPY node/target/wasm32-unknown-unknown/release/bitblock_light_wasm.wasm /app/bitblock_light_wasm.wasm

ENV WASM_PATH=/app/bitblock_light_wasm.wasm
ENV HEALTH_HOST=0.0.0.0
ENV HEALTH_PORT=8690

EXPOSE 8690
HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -qO- http://127.0.0.1:8690/health || exit 1

ENTRYPOINT ["node", "/app/index.mjs"]
