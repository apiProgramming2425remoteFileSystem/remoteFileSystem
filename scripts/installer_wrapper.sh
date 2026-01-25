#!/bin/sh
set -e

# Usage: ./install_wrapper.sh <PATH_LOGIC> <REAL_BIN_NAME> <WRAPPER_OUTPUT_PATH>
#
# Arguments:
#   1. PATH_LOGIC: The string to put after 'cd'. 
#      - For Docker: Use an absolute path (e.g., "/opt/app")
#      - For Local:  Use a dynamic shell command (e.g., "$(dirname "$0")/.core")
#   2. REAL_BIN_NAME: The name of the binary to run (e.g., "remote_fs_client_core")
#   3. WRAPPER_OUTPUT_PATH: Where to save the wrapper script

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 <PATH_LOGIC> <REAL_BIN_NAME> <WRAPPER_OUTPUT_PATH>"
    exit 1
fi

PATH_LOGIC="$1"
REAL_BIN_NAME="$2"
WRAPPER_OUTPUT_PATH="$3"

# Create the directory for the wrapper if it doesn't exist (just in case)
mkdir -p "$(dirname "$WRAPPER_OUTPUT_PATH")"

# Generate the wrapper script
cat > "$WRAPPER_OUTPUT_PATH" <<'EOF'
#!/bin/sh
# Auto-generated wrapper script

# Navigate to the installation directory
EOF

# Append the dynamic directory logic
echo "cd \"$PATH_LOGIC\"" >> "$WRAPPER_OUTPUT_PATH"

# Finish the script
cat >> "$WRAPPER_OUTPUT_PATH" <<EOF    
# Execute the real binary, passing all arguments
exec ./$REAL_BIN_NAME "\$@"
EOF

chmod +x "$WRAPPER_OUTPUT_PATH"