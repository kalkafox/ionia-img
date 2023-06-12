FROM rust:alpine

# add gcc and musl-dev for compiling native dependencies
RUN apk add --no-cache gcc musl-dev

RUN mkdir -p /build
WORKDIR /build

COPY . /build/

RUN cargo build --release

FROM node

RUN mkdir -p /app
WORKDIR /app

COPY ./frontend /app/

RUN npm install
RUN npm run build

FROM scratch

COPY --from=1 /app/dist /frontend/dist
COPY --from=0 /build/target/release/lilith-upload /bin/lilith-upload

EXPOSE 3030

CMD ["/bin/lilith-upload"]