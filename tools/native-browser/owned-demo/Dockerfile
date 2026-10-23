# Source recipe only: no image was built or pushed during local qualification.
# Both arguments must be reviewed digest-pinned Docker Official Images. Use
# compatible Linux/amd64 Debian versions; never silently choose floating tags.
ARG RUST_IMAGE
ARG NODE_IMAGE
FROM ${RUST_IMAGE} AS native
WORKDIR /src
COPY . .
RUN rustc --version | grep '^rustc 1.99.0 '
RUN cargo build --locked --release -p wonderland-native-lot-host

FROM ${NODE_IMAGE} AS runtime
WORKDIR /app
COPY --from=native /src/target/release/wonderland-native-lot-host ./bin/wonderland-native-lot-host
COPY services/native-edge/package.json services/native-edge/package-lock.json ./services/native-edge/
RUN node --version | grep '^v26\.'
RUN cd services/native-edge && npm ci --ignore-scripts --omit=dev --omit=optional
COPY tools/native-browser/owned-demo.mjs ./tools/native-browser/owned-demo.mjs
COPY tools/native-browser/owned-demo ./tools/native-browser/owned-demo
COPY web/index.html ./web/index.html
USER node
EXPOSE 8080
CMD ["node", "tools/native-browser/owned-demo/container-main.mjs"]
