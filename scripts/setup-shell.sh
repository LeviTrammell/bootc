#!/usr/bin/env bash

set -euo pipefail

# Ensure root home directory exists and has correct permissions
mkdir -p /root 2>/dev/null || true

# Install oh-my-zsh for root
sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended

# Create skel directory for new users
mkdir -p /etc/skel

# Install oh-my-zsh template for new users
git clone --depth=1 https://github.com/ohmyzsh/ohmyzsh.git /etc/skel/.oh-my-zsh

# Configure root's .zshrc
cat > /root/.zshrc << 'EOF'
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

# Configure template .zshrc for new users
cat > /etc/skel/.zshrc << 'EOF'
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

# Set zsh as default shell
chsh -s /usr/bin/zsh root

echo "Shell setup complete!"
