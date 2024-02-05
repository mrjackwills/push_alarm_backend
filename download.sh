#!/bin/bash

case "$(arch)" in
x86_64) SUFFIX="x86_64" ;;
aarch64) SUFFIX="aarch64" ;;
armv6l) SUFFIX="armv6" ;;
esac

if [ -n "$SUFFIX" ]; then
	PUSH_ALARM_GZ="push_alarm_linux_${SUFFIX}.tar.gz"
	wget "https://github.com/mrjackwills/psuh_alarm_backend/releases/latest/download/${PUSH_ALARM_GZ}"
	tar xzvf "${PUSH_ALARM_GZ}" push_alarm
	rm "${PUSH_ALARM_GZ}"
fi
