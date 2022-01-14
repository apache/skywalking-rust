FROM rust:1.57 as build
WORKDIR /build
COPY . /build/
RUN cd tests/e2e && cargo build --release --locked
ENTRYPOINT ["/build/tests/e2e/target/release/e2e"]
