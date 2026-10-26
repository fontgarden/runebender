#!/bin/sh
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

# Local test builds only. Use package_macos.py directly for Developer ID signing.
set -eu
script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
exec python3 "$script_dir/package_macos.py" --unsigned "$@"
