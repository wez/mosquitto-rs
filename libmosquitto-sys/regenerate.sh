#!/bin/bash

bindgen binding.h -o src/lib.rs \
  --no-layout-tests \
  --no-doc-comments \
  --raw-line "#![allow(non_snake_case)]" \
  --raw-line "#![allow(non_camel_case_types)]" \
  --raw-line "#![allow(non_upper_case_globals)]" \
  --raw-line "#![allow(clippy::unreadable_literal)]" \
  --default-enum-style rust \
  --generate=functions,types,vars \
  --allowlist-function="(mqtt|mosq).*" \
  --allowlist-type="(mqtt|mosq).*" \
  -- -Imosquitto/include
