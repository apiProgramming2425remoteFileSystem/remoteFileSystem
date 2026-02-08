#!/bin/bash
set -e # Exit immediately on error

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
PROJECT_ROOT="$(readlink -f "$SCRIPT_DIR/../../..")"
BUILD_DIR="$PROJECT_ROOT/build_output/unix/client" 

# Configurations
SOURCE_BINARY="$PROJECT_ROOT/target/release/client"  # The compiled binary location
APP_NAME="remote_fs_client"                          # The name of the final command user runs
REAL_BIN_NAME="remote_fs_client_core"                # The name of the hidden binary

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

install_dependencies() {
    # Detect OS and Install Dependencies
    OS="$(uname -s)"
    log_info "Detected OS: $OS."

    case "${OS}" in
        Linux*)
            if [ -f /etc/debian_version ]; then
                # Ubuntu / Debian / Mint
                log_info "Debian/Ubuntu detected."
                PACKAGES=("pkg-config" "fuse3" "libfuse3-dev" "libssl-dev" "libfontconfig1" "libfontconfig1-dev" "libx11-6" "libx11-dev" "libxcursor-dev" "libxi-dev" "libxrandr-dev" "libxinerama-dev" "libxkbcommon-x11-0" "libxkbcommon-x11-dev" "fonts-dejavu-core" "libgles2-mesa-dev" "libegl1-mesa-dev" "ca-certificates")
                install_apt_packages "${PACKAGES[@]}"

            # elif [ -f /etc/redhat-release ]; then
            #     # Fedora / RHEL / CentOS
            #     log_info "RHEL/Fedora detected."
            #     sudo dnf install -y gcc pkgconf-pkg-config fuse3-devel fuse3 openssl-devel

            # elif [ -f /etc/arch-release ]; then
            #     # Arch Linux
            #     log_info "Arch Linux detected.."
            #     sudo pacman -S --needed base-devel fuse3 openssl

            else
                log_info "Unknown Linux distribution. Skipping automatic dependency installation."
                log_info "Please ensure the following packages are installed: pkg-config, fuse3, openssl"
            fi
            ;;
        Darwin*)
            # macOS
            PACKAGES=("pkg-config" "macfuse" "openssl@3")
            install_brew_packages "${PACKAGES[@]}"
            ;;
        *)
            log_error "Unsupported OS: ${OS}. This script supports only Linux and macOS."
            exit 1
            ;;
    esac
}

# Main Script Execution
log_info "Starting Build Process for $APP_NAME..."

check_cargo
install_dependencies
build_binary "client"

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
