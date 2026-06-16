#!/bin/sh
set -e

# Generate runtime config JS file from environment variables
# This runs at container startup, before Nginx starts serving
cat > /usr/share/nginx/html/env-config.js << EOF
window.__RUNTIME_CONFIG__ = {
  VITE_API_BASE_URL: "${VITE_API_BASE_URL:-http://backend:5000}",
};
EOF
