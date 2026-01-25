#!/bin/bash
set -e # Exit immediately on error

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
PROJECT_ROOT="$(readlink -f "$SCRIPT_DIR/../../..")"
BUILD_DIR="$PROJECT_ROOT/build_output/unix/server" 

# Configurations
SOURCE_BINARY="$PROJECT_ROOT/target/release/server"  # The compiled binary location
APP_NAME="remote_fs_server"                          # The name of the final command user runs
REAL_BIN_NAME="remote_fs_server_core"                # The name of the hidden binary

# Helpers Scripts Locations
UTILS_SCRIPT=$(readlink -f "$SCRIPT_DIR/../utils.sh")
HELPER_SCRIPT="$PROJECT_ROOT/scripts/installer_wrapper.sh"

# Check if utils exist, then source them
if [ -f "$UTILS_SCRIPT" ]; then
    source "$UTILS_SCRIPT"
else
    echo "Error: Cannot find utils.sh at $UTILS_SCRIPT"
    exit 1
fi

# Main Script Execution
log_info "Starting Build Process for $APP_NAME..."

check_cargo
build_binary "server"

# Package args: <Source> <Output Folder> <App Name> <Bin Name> <Helper Script>
package_portable_app \
    "$SOURCE_BINARY" \
    "$BUILD_DIR" \
    "$APP_NAME" \
    "$REAL_BIN_NAME" \
    "$HELPER_SCRIPT"

log_success "Build Completed Successfully!"

echo ""
echo "  Artifact Location: $BUILD_DIR"
echo "  Executable:        $BUILD_DIR/$APP_NAME"
echo ""
echo "  [Tip] You can copy the entire '$BUILD_DIR' folder anywhere."
# echo "        Running '$APP_NAME' will always keep logs inside '.core/logs'."

# log_info "Binary is ready at: $(pwd)$BIN_DEST"
# log_info "To run it, make sure to execute:" 
# log_info "   chmod +x $BIN_DEST"