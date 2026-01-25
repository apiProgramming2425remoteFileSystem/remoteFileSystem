#!/bin/bash
set -e # Exit immediately on error

# Define colors
COLOR_INFO="\033[1;34m"
COLOR_ERROR="\033[1;31m"
COLOR_SUCCESS="\033[1;32m"
COLOR_RESET="\033[0m"

# Logging Functions
log_info() {
    echo -e "${COLOR_INFO}[INFO]${COLOR_RESET} $1"
}
log_error() {
    echo -e "${COLOR_ERROR}[ERROR]${COLOR_RESET} $1"
}
log_success() {
    echo -e "${COLOR_SUCCESS}[SUCCESS]${COLOR_RESET} $1"
}

# Check for Cargo Installation
check_cargo() {
    if ! command -v cargo >/dev/null 2>&1; then
        log_error "Rust/Cargo is not installed. Please install Rust: https://www.rust-lang.org/tools/install"
        exit 1
    fi
}

# Generic function to check and install APT packages (Debian/Ubuntu)
install_apt_packages() {
    # Read all arguments into an array
    local REQUIRED_PKGS=("$@")
    local MISSING_PKGS=()

    log_info "Checking dependencies: ${REQUIRED_PKGS[*]}..."

    # Check which packages are actually missing
    for pkg in "${REQUIRED_PKGS[@]}"; do
        if ! dpkg -s "$pkg" >/dev/null 2>&1; then
            MISSING_PKGS+=("$pkg")
        fi
    done

    # Install only if necessary
    if [ ${#MISSING_PKGS[@]} -eq 0 ]; then
        log_success "All dependencies are already installed."
    else
        log_info "Missing packages: ${MISSING_PKGS[*]}"
        log_info "Installing..."

        # Handle Root vs Sudo automatically
        if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then
            sudo apt-get update
            sudo apt-get install -y "${MISSING_PKGS[@]}"
        elif [ "$(id -u)" -eq 0 ]; then
            apt-get update
            apt-get install -y "${MISSING_PKGS[@]}"
        else
            log_error "Root privileges required to install: ${MISSING_PKGS[*]}"
            log_error "'sudo' command not found. Please install the required packages manually:"
            echo "  ${MISSING_PKGS[*]}"
            exit 1
        fi
        log_success "Dependencies installed successfully."
    fi
}

# Generic function for Homebrew (macOS)
install_brew_packages() {
    local REQUIRED_PKGS=("$@")
    
    if ! command -v brew >/dev/null; then
        log_error "Homebrew not found. Please install: ${REQUIRED_PKGS[*]}"
        exit 1
    fi

    for pkg in "${REQUIRED_PKGS[@]}"; do
        if ! brew list --formula "$pkg" >/dev/null 2>&1; then
            log_info "Installing $pkg..."
            brew install "$pkg"
        else
            log_success "$pkg is already installed."
        fi
    done
}

# Build the Project
build_binary() {
    local BINARY="$1"

    log_info "Building Release Binary for $BINARY..."

    # Navigate to Project Root
    cd "$PROJECT_ROOT"
    cargo build --release --bin "$BINARY"
}


# Generic function to package a binary into a portable structure
# Usage: package_portable_app <SOURCE_BIN> <OUTPUT_DIR> <APP_NAME> <REAL_BIN_NAME> <HELPER_SCRIPT_PATH>
package_portable_app() {
    # Prepare Directory Structure
    # 
    # <OUTPUT_DIR>/
    #   <APP_NAME> (Wrapper Script)
    #   <CORE_DIR>/
    #      <REAL_BIN_NAME> (Real Binary)

    local SOURCE_BIN="$1"
    local OUTPUT_DIR="$2"
    local APP_NAME="$3"
    local REAL_BIN_NAME="$4"
    local HELPER_SCRIPT="$5"
    
    local CORE_DIR=".rfs_core"
    local DEST_CORE="$OUTPUT_DIR/$CORE_DIR"
    local WRAPPER_OUTPUT="$OUTPUT_DIR/$APP_NAME"

    log_info "Packaging '$APP_NAME'..."

    # Validation
    if [ ! -f "$SOURCE_BIN" ]; then
        log_error "Source binary not found at: $SOURCE_BIN"
        exit 1
    fi
    if [ ! -f "$HELPER_SCRIPT" ]; then
        log_error "Helper script not found at: $HELPER_SCRIPT"
        exit 1
    fi

    # Prepare Directories
    mkdir -p "$DEST_CORE"

    # Copy the real binary to the hidden core folder
    cp "$SOURCE_BIN" "$DEST_CORE/$REAL_BIN_NAME"
    chmod +x "$DEST_CORE/$REAL_BIN_NAME"

    # Generate the Wrapper using the helper script
    # We pass the DYNAMIC path logic so it works relatively
    local DYNAMIC_PATH='$(dirname "$0")/'"$CORE_DIR"
    
    bash "$HELPER_SCRIPT" "$DYNAMIC_PATH" "$REAL_BIN_NAME" "$WRAPPER_OUTPUT"
}