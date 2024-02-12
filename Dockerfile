#########
# SETUP #
#########

FROM alpine:3.19 as SETUP

# This should get automatically updated on every release via create_release.sh
# DO NOT EDIT MANUALLY
ARG CURRENT_VERSION=0.1.0

ARG DOCKER_APP_USER=app_user \
    DOCKER_APP_GROUP=app_group

ENV VIRT=".build_packages"
ENV TZ=${DOCKER_TIME_CONT}/${DOCKER_TIME_CITY}

WORKDIR /app

RUN addgroup -S ${DOCKER_APP_GROUP} \
    && adduser -S -G ${DOCKER_APP_GROUP} ${DOCKER_APP_USER} \
    && apk --no-cache add --virtual ${VIRT} ca-certificates \
    && apk del ${VIRT} \
    && mkdir /db_data \
    && chown ${DOCKER_APP_USER}:${DOCKER_APP_GROUP} /db_data

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

FROM scratch AS RUNNER

ARG DOCKER_APP_USER=app_user \
    DOCKER_APP_GROUP=app_group

COPY --from=SETUP /app/ /app
COPY --from=SETUP /etc/group /etc/passwd /etc/
COPY --from=SETUP /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

COPY --from=SETUP --chown=${DOCKER_APP_USER}:${DOCKER_APP_GROUP} /db_data /db_data

USER ${DOCKER_APP_USER}

ENTRYPOINT ["/app/push_alarm"]