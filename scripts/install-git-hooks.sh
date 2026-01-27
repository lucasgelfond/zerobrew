#!/bin/bash
set -e

# Install git pre-commit hooks for code quality checks

HOOKS_DIR=".git/hooks"

if [ ! -d "$HOOKS_DIR" ]; then
    echo "Error: .git/hooks directory not found. Are you in a git repository?"
    exit 1
fi

echo "Installing pre-commit hook..."

cat > "$HOOKS_DIR/pre-commit" << 'EOF'
#!/bin/bash
set -e

echo "Running pre-commit checks..."

# Check formatting
echo "Checking code formatting..."
if ! cargo fmt --all -- --check; then
    echo ""
    echo "❌ Code formatting check failed!"
    echo "Run 'cargo fmt --all' to fix formatting issues."
    echo ""
    echo "To skip this check, use: git commit --no-verify"
    exit 1
fi

# Run clippy
echo "Running clippy..."
if ! cargo clippy --all-targets --all-features -- -D warnings; then
    echo ""
    echo "❌ Clippy found issues!"
    echo "Fix the issues above before committing."
    echo ""
    echo "To skip this check, use: git commit --no-verify"
    exit 1
fi

echo "✅ All pre-commit checks passed!"
EOF

chmod +x "$HOOKS_DIR/pre-commit"

echo "✅ Pre-commit hook installed successfully!"
echo ""
echo "The hook will run 'cargo fmt' and 'cargo clippy' before each commit."
echo "To skip the hook for a specific commit, use: git commit --no-verify"
