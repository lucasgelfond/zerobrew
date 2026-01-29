#!/bin/bash
# Install shell completions for zb

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

SHELL_NAME=$(basename "$SHELL")

echo -e "${CYAN}==> Installing zb shell completions${NC}"
echo ""

case "$SHELL_NAME" in
  zsh)
    echo "Detected zsh shell"
    
    # Find zsh completions directory
    ZSH_COMPLETIONS_DIR="${ZDOTDIR:-$HOME}/.zsh/completions"
    
    # Create directory if it doesn't exist
    mkdir -p "$ZSH_COMPLETIONS_DIR"
    
    # Generate completion
    zb completion zsh > "$ZSH_COMPLETIONS_DIR/_zb"
    echo -e "${GREEN}✓${NC} Generated zsh completions at $ZSH_COMPLETIONS_DIR/_zb"
    
    # Add completion function to shell config if not already there
    ZSHENV="${ZDOTDIR:-$HOME}/.zshenv"
    if ! grep -q "fpath=.*\.zsh/completions" "$ZSHENV" 2>/dev/null; then
      echo "" >> "$ZSHENV"
      echo "# zb completions" >> "$ZSHENV"
      echo "fpath=(${ZDOTDIR:-\$HOME}/.zsh/completions \$fpath)" >> "$ZSHENV"
      echo "autoload -Uz compinit && compinit" >> "$ZSHENV"
      echo -e "${GREEN}✓${NC} Added completion function to $ZSHENV"
    else
      echo -e "${YELLOW}→${NC} Completion function already in $ZSHENV"
    fi
    
    echo ""
    echo "To enable completions, run:"
    echo -e "  ${CYAN}source $ZSHENV${NC}"
    ;;
    
  bash)
    echo "Detected bash shell"
    
    # Find bash completions directory
    if [[ -d /opt/homebrew/etc/bash_completion.d ]]; then
      BASH_COMPLETIONS_DIR="/opt/homebrew/etc/bash_completion.d"
    elif [[ -d /usr/local/etc/bash_completion.d ]]; then
      BASH_COMPLETIONS_DIR="/usr/local/etc/bash_completion.d"
    elif [[ -d ~/.bash_completion.d ]]; then
      BASH_COMPLETIONS_DIR="~/.bash_completion.d"
    else
      BASH_COMPLETIONS_DIR="$HOME/.bash_completion.d"
    fi
    
    # Create directory if it doesn't exist
    mkdir -p "$BASH_COMPLETIONS_DIR"
    
    # Generate completion
    zb completion bash > "$BASH_COMPLETIONS_DIR/zb"
    echo -e "${GREEN}✓${NC} Generated bash completions at $BASH_COMPLETIONS_DIR/zb"
    
    # Add sourcing to bashrc if not already there
    BASHRC="$HOME/.bashrc"
    if [[ ! -f "$BASHRC" ]]; then
      BASHRC="$HOME/.bash_profile"
    fi
    
    if ! grep -q "bash_completion.d/zb" "$BASHRC" 2>/dev/null; then
      echo "" >> "$BASHRC"
      echo "# zb completions" >> "$BASHRC"
      echo "source $BASH_COMPLETIONS_DIR/zb" >> "$BASHRC"
      echo -e "${GREEN}✓${NC} Added completion source to $BASHRC"
    else
      echo -e "${YELLOW}→${NC} Completion already sourced in $BASHRC"
    fi
    
    echo ""
    echo "To enable completions, run:"
    echo -e "  ${CYAN}source $BASHRC${NC}"
    ;;
    
  fish)
    echo "Detected fish shell"
    
    FISH_COMPLETIONS_DIR="$HOME/.config/fish/completions"
    mkdir -p "$FISH_COMPLETIONS_DIR"
    
    # Generate completion
    zb completion fish > "$FISH_COMPLETIONS_DIR/zb.fish"
    echo -e "${GREEN}✓${NC} Generated fish completions at $FISH_COMPLETIONS_DIR/zb.fish"
    
    echo ""
    echo "Completions will be automatically loaded next time you start fish."
    ;;
    
  *)
    echo -e "${YELLOW}Warning:${NC} Unsupported shell: $SHELL_NAME"
    echo ""
    echo "Supported shells: zsh, bash, fish"
    echo ""
    echo "You can manually generate completions with:"
    echo -e "  ${CYAN}zb completion bash${NC}  # for bash"
    echo -e "  ${CYAN}zb completion zsh${NC}   # for zsh"
    echo -e "  ${CYAN}zb completion fish${NC}  # for fish"
    exit 1
    ;;
esac

echo ""
echo -e "${GREEN}==> Completion installation complete!${NC}"
