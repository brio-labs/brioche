#!/bin/bash
set -e

echo "Checking GPG configuration..."

if ! gpg --list-secret-keys --keyid-format LONG | grep -q sec; then
    echo "ERROR: No GPG secret keys found."
    echo "Generate one with: gpg --full-generate-key"
    exit 1
fi

KEY_ID=$(gpg --list-secret-keys --keyid-format LONG | grep sec | head -1 | awk '{print $2}' | cut -d'/' -f2)
echo "Found GPG key: $KEY_ID"

git config user.signingkey "$KEY_ID"
git config commit.gpgsign true
git config tag.gpgsign true

echo "GPG signing configured for this repository."
