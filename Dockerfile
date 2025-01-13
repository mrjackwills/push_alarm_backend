#########
# SETUP #
#########

FROM alpine:3.19 AS setup

# This should get automatically updated on every release via create_release.sh
# DO NOT EDIT MANUALLY
ARG CURRENT_VERSION=0.2.4

ARG DOCKER_APP_USER=app_user \
    DOCKER_APP_GROUP=app_group \
    DOCKER_GUID=1000 \
    DOCKER_UID=1000

ENV VIRT=".build_packages"

WORKDIR /app

RUN addgroup -g ${DOCKER_GUID} -S ${DOCKER_APP_GROUP} \
    && adduser -u ${DOCKER_UID} -S -G ${DOCKER_APP_GROUP} ${DOCKER_APP_USER} \
    && apk --no-cache add --virtual ${VIRT} ca-certificates \
    && apk del ${VIRT}

# Somewhat convoluted way to automatically select & download the correct package
RUN ARCH=$(uname -m) && \
    case "$ARCH" in \
        x86_64) SUFFIX=x86_64 ;; \
        aarch64) SUFFIX=aarch64 ;; \
        armv6l) SUFFIX=armv6 ;; \
        *) exit 1 ;; \
    esac \ 
    && wget https://github.com/mrjackwills/push_alarm_backend/releases/download/v${CURRENT_VERSION}/push_alarm_linux_${SUFFIX}.tar.gz \
    && tar xzvf push_alarm_linux_${SUFFIX}.tar.gz push_alarm \
    && rm push_alarm_linux_${SUFFIX}.tar.gz \
    && chown ${DOCKER_APP_USER}:${DOCKER_APP_GROUP} /app/push_alarm

##########
# RUNNER #
##########

FROM scratch

ARG DOCKER_APP_USER=app_user

COPY --from=setup /app/ /app
COPY --from=setup /etc/group /etc/passwd /etc/
COPY --from=setup /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

USER ${DOCKER_APP_USER}

ENTRYPOINT ["/app/push_alarm"]