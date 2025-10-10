#!/usr/bin/env bash

set -euo pipefail

echo "Initializing shell environment for user: $USER"

# Check if oh-my-zsh is already installed
if [ ! -d "$HOME/.oh-my-zsh" ]; then
    echo "Installing oh-my-zsh..."
    sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended
else
    echo "oh-my-zsh already installed"
fi

# Backup existing .zshrc if it exists
if [ -f "$HOME/.zshrc" ] && [ ! -f "$HOME/.zshrc.backup" ]; then
    echo "Backing up existing .zshrc to .zshrc.backup"
    cp "$HOME/.zshrc" "$HOME/.zshrc.backup"
fi

# Copy the standard configuration
echo "Setting up .zshrc configuration..."
cat > "$HOME/.zshrc" << 'EOF'
# Path to oh-my-zsh installation
export ZSH="$HOME/.oh-my-zsh"

# Theme
ZSH_THEME="robbyrussell"

# Plugins
plugins=(git)

source $ZSH/oh-my-zsh.sh

# Zoxide initialization
eval "$(zoxide init zsh)"

# Alias z to zoxide
alias z='zoxide'

# Auto-start zellij on SSH login
if [[ -n "$SSH_CONNECTION" ]] && [[ -z "$ZELLIJ" ]]; then
  exec zellij attach --create default
fi
EOF

# Change default shell to zsh if not already
if [ "$SHELL" != "$(which zsh)" ]; then
    echo "Changing default shell to zsh..."
    chsh -s "$(which zsh)"
    echo "Shell changed to zsh. Please log out and back in for it to take effect."
else
    echo "Default shell is already zsh"
fi

echo ""
echo "✓ Shell environment initialized successfully!"
echo "  - zsh with oh-my-zsh installed"
echo "  - zoxide configured with 'z' alias"
echo "  - zellij auto-start on SSH enabled"
echo ""
echo "Start using zsh now with: exec zsh"
