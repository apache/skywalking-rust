FROM rust:1.57 as build
WORKDIR /build
COPY . /build/
RUN cd tests/e2e && cargo build --release --locked

FROM gcr.io/distroless/cc
COPY --from=build /build/tests/e2e/target/release/e2e /bin/
ENTRYPOINT ["/bin/e2e"]
