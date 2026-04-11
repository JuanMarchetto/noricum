#!/usr/bin/env bash
# Back up Claude Code agent-memory directory to a tarball.
#
# The agent-memory directory at ~/.claude/projects/{slug}/memory/ holds
# per-project memories that survive conversation boundaries. Those files
# are NOT in the noricum repo, so if the machine dies, they die too.
#
# This script produces a timestamped tarball of the memory directory plus
# a JSON manifest listing every file and its sha256. Default output is
# ~/noricum-memory-backups/{timestamp}.tar.gz, but override with $1.
#
# To restore on a new machine:
#   mkdir -p ~/.claude/projects/-home-marche-noricum/memory
#   tar xzvf {backup}.tar.gz -C ~/.claude/projects/-home-marche-noricum/
#
# Recommended: run this weekly from cron, or after any session where
# non-trivial learnings were added to memory:
#   0 20 * * 0 /home/marche/noricum/tools/backup-agent-memory.sh
#
# Optional: pipe the tarball to a private GitHub repo, cloud storage, or
# a second machine for off-site backup. This script does not assume any
# specific transport.

set -euo pipefail

MEMORY_DIR="${MEMORY_DIR:-$HOME/.claude/projects/-home-marche-noricum/memory}"
BACKUP_ROOT="${1:-$HOME/noricum-memory-backups}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_FILE="$BACKUP_ROOT/memory-$TIMESTAMP.tar.gz"
MANIFEST="$BACKUP_ROOT/memory-$TIMESTAMP.manifest.json"

if [ ! -d "$MEMORY_DIR" ]; then
    echo "ERROR: memory dir not found at $MEMORY_DIR" >&2
    exit 1
fi

mkdir -p "$BACKUP_ROOT"

# Build a JSON manifest listing every file + sha256.
{
    echo "{"
    echo "  \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\","
    echo "  \"source\": \"$MEMORY_DIR\","
    echo "  \"files\": ["
    first=1
    while IFS= read -r -d '' f; do
        rel="${f#$MEMORY_DIR/}"
        sha=$(sha256sum "$f" | awk '{print $1}')
        size=$(stat -c %s "$f")
        [ $first -eq 1 ] && first=0 || echo ","
        printf '    {"path": "%s", "sha256": "%s", "size": %d}' "$rel" "$sha" "$size"
    done < <(find "$MEMORY_DIR" -type f -print0 | sort -z)
    echo ""
    echo "  ]"
    echo "}"
} > "$MANIFEST"

# Tar the memory directory with compression.
tar -C "$(dirname "$MEMORY_DIR")" -czf "$BACKUP_FILE" "$(basename "$MEMORY_DIR")"

BYTES=$(stat -c %s "$BACKUP_FILE")
COUNT=$(find "$MEMORY_DIR" -type f | wc -l)

echo "Memory backup complete:"
echo "  Files:    $COUNT"
echo "  Archive:  $BACKUP_FILE ($BYTES bytes)"
echo "  Manifest: $MANIFEST"
echo ""
echo "To restore on a new machine:"
echo "  mkdir -p ~/.claude/projects/-home-marche-noricum/"
echo "  tar xzvf $(basename "$BACKUP_FILE") -C ~/.claude/projects/-home-marche-noricum/"
echo ""
echo "Consider syncing $BACKUP_ROOT to off-site storage:"
echo "  rsync -a $BACKUP_ROOT/ user@backup-server:~/noricum-memory-backups/"
echo "  OR"
echo "  gh repo create --private JuanMarchetto/noricum-memory-backup (one-time)"
echo "  cd $BACKUP_ROOT && git init && git add . && git commit -m 'backup $TIMESTAMP' && git push"
