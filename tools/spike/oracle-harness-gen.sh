#!/usr/bin/env bash
# spike-oracle-harness-gen — auto-generate wrapper.c stubs from a C header file.
#
# Given a target C header, parses the function declarations and emits:
#   - wrapper.h with matching opaque-handle functions
#   - wrapper.c with thin delegation stubs (TODO markers inside)
#   - src/lib.rs extern block updates
#
# This is a GREP-based parser, not a proper AST. It handles the common
# case (single-line declarations with standard C types) and flags anything
# it couldn't parse with a comment so the human can finish it.
#
# Usage:
#   tools/spike/oracle-harness-gen.sh <spike-dir> <header-file> [filter-regex]
#
# Example:
#   tools/spike/oracle-harness-gen.sh \
#       noricum-spike-libpng \
#       noricum-spike-libpng/libpng.h \
#       '^png_(create|destroy|read|write)'
#
# If filter-regex is provided, only function names matching it are wrapped.
# Otherwise, all top-level function declarations are wrapped.

set -euo pipefail

SPIKE_DIR="${1:-}"
HEADER="${2:-}"
FILTER="${3:-.*}"

if [ -z "$SPIKE_DIR" ] || [ -z "$HEADER" ]; then
    echo "Usage: $0 <spike-dir> <header-file> [filter-regex]" >&2
    exit 1
fi
if [ ! -d "$SPIKE_DIR" ] || [ ! -f "$HEADER" ]; then
    echo "ERROR: spike dir or header not found" >&2
    exit 1
fi

WRAPPER_H="$SPIKE_DIR/wrapper.h"
WRAPPER_C="$SPIKE_DIR/wrapper.c"

# Grep out function declarations. Common patterns:
#   <ret> <name>(<args>);
#   EXPORT_MACRO <ret> <name>(<args>);
# Skip typedef, struct, static, extern-block openers.
DECLS=$(awk '
    /typedef|struct \{|^\s*\/\*|^\s*\/\/|^\s*#/ { next }
    /\(/ && /\)/ && /;/ && !/^\s*static/ {
        # Remove leading EXPORT-like macros.
        gsub(/^[ \t]*(MINIZ_EXPORT|EXPORT|API|extern|DLL[A-Z_]*)[ \t]+/, "")
        # Collapse whitespace.
        gsub(/[ \t]+/, " ")
        print
    }
' "$HEADER" | grep -E "\b\w+\s*\(" | grep -E "$FILTER" || true)

if [ -z "$DECLS" ]; then
    echo "No function declarations found matching /$FILTER/ in $HEADER"
    exit 0
fi

COUNT=$(echo "$DECLS" | wc -l)
echo "Found $COUNT function declarations matching /$FILTER/"

# Generate wrapper.h stub entries.
{
    echo ""
    echo "/* ---- auto-generated wrapper stubs from $(basename "$HEADER") ---- */"
    echo "/* Review each signature and adjust as needed. The generator is"
    echo " * grep-based, not an AST parser, so complex C types may be mangled. */"
    echo ""
    i=0
    while IFS= read -r decl; do
        # Extract function name.
        fname=$(echo "$decl" | grep -oE '\b[a-zA-Z_][a-zA-Z0-9_]*\s*\(' | head -1 | tr -d ' (')
        [ -z "$fname" ] && continue
        i=$((i + 1))
        echo "/* #$i: $decl */"
        echo "/* TODO: define a wrapper for $fname in wrapper.c */"
        echo "/* int wr_${fname}(/* opaque + relevant args */); */"
        echo ""
    done <<< "$DECLS"
    echo "/* ---- end auto-generated ---- */"
} >> "$WRAPPER_H"

echo "Appended $COUNT stub comments to $WRAPPER_H"
echo ""
echo "Next steps:"
echo "  1. Open $WRAPPER_H and convert each TODO to a real wrapper signature"
echo "  2. Implement matching functions in $WRAPPER_C"
echo "  3. Add matching extern declarations in $SPIKE_DIR/src/lib.rs"
echo "  4. Run 'cargo check' to verify the FFI layer compiles"
echo ""
echo "Note: the grep parser only handles simple single-line declarations."
echo "Multi-line declarations or complex macros will not be wrapped."
echo "For those, copy the signature by hand."
