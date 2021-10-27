#!/bin/sh

parameter=""

if [ ! -z "$LISTEN" ]
then
      parameter="$parameter --listen $LISTEN"
fi

if [ ! -z "$TARGET" ]
then
      parameter="$parameter --target $TARGET"
fi

if [ ! -z "$PROXY" ]
then
      parameter="$parameter --proxy-addr $PROXY"
fi

if [ ! -z "$USERNAME" ]
then
      parameter="$parameter --proxy-user $USERNAME"
fi

if [ ! -z "$PASSWORD" ]
then
      parameter="$parameter --proxy-pass $PASSWORD"
fi

socks5-forwarder $parameter
