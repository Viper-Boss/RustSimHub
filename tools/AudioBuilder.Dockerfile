FROM rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa
LABEL org.rustsimhub.purpose="audio-development-build"
COPY *.deb /tmp/audio-deps/
COPY SHA256SUMS /tmp/audio-deps/
RUN cd /tmp/audio-deps && test "$(find . -name '*.deb' | wc -l)" = 7 && sha256sum -c SHA256SUMS && dpkg -i ./*.deb && rm -rf /tmp/audio-deps
