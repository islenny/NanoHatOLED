#!/usr/bin/env bash
set -euo pipefail

BASE_REF="AUTO"
TARGET_FILE="README.md"
EMPTY_TREE="4b825dc642cb6eb9a060e54bf8d69288fbee4904"

usage() {
  cat <<'EOF'
Usage: ./scripts/preview-readme-changes.sh [--base <git-ref>] [--file <path>]

Preview the Markdown sections in the current working tree that were touched
relative to a git base revision.

Options:
  --base <git-ref>  Compare against this revision (default: auto)
  --file <path>     Markdown file to inspect (default: README.md)
  -h, --help        Show this help text
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base)
      [[ $# -ge 2 ]] || {
        echo "Missing value for --base" >&2
        exit 1
      }
      BASE_REF="$2"
      shift 2
      ;;
    --file)
      [[ $# -ge 2 ]] || {
        echo "Missing value for --file" >&2
        exit 1
      }
      TARGET_FILE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "This script must run inside a git work tree." >&2
  exit 1
fi

if [[ ! -f "$TARGET_FILE" ]]; then
  echo "File not found: $TARGET_FILE" >&2
  exit 1
fi

DIFF_BASE=""
BASE_LABEL=""
if [[ "$BASE_REF" == "AUTO" ]]; then
  if git rev-parse --verify "HEAD^{commit}" >/dev/null 2>&1; then
    DIFF_BASE="HEAD"
    BASE_LABEL="HEAD"
    DIFF_OUTPUT="$(git diff --no-ext-diff --unified=0 HEAD -- "$TARGET_FILE")"
  elif git ls-files --error-unmatch -- "$TARGET_FILE" >/dev/null 2>&1; then
    BASE_LABEL="index"
    DIFF_OUTPUT="$(git diff --no-ext-diff --unified=0 -- "$TARGET_FILE")"
  else
    DIFF_BASE="$EMPTY_TREE"
    BASE_LABEL="empty tree"
    DIFF_OUTPUT="$(git diff --no-ext-diff --unified=0 "$DIFF_BASE" -- "$TARGET_FILE")"
  fi
else
  DIFF_BASE="$BASE_REF"
  BASE_LABEL="$BASE_REF"
  if ! git rev-parse --verify "$BASE_REF^{commit}" >/dev/null 2>&1; then
    if [[ "$BASE_REF" == "HEAD" ]]; then
      DIFF_BASE="$EMPTY_TREE"
      BASE_LABEL="HEAD (empty tree)"
    else
      echo "Invalid git base ref: $BASE_REF" >&2
      exit 1
    fi
  fi

  DIFF_OUTPUT="$(git diff --no-ext-diff --unified=0 "$DIFF_BASE" -- "$TARGET_FILE")"
fi
if [[ -z "$DIFF_OUTPUT" ]]; then
  echo "No changes detected in $TARGET_FILE relative to $BASE_REF."
  exit 0
fi

mapfile -t CHANGED_LINES < <(
  printf '%s\n' "$DIFF_OUTPUT" | awk '
    /^@@ / {
      if (match($0, /\+[0-9]+(,[0-9]+)?/)) {
        token = substr($0, RSTART + 1, RLENGTH - 1)
        split(token, parts, ",")
        start = parts[1] + 0
        count = (length(parts) > 1 ? parts[2] + 0 : 1)

        if (count == 0) {
          print (start > 0 ? start : 1)
        } else {
          for (i = 0; i < count; i++) {
            print start + i
          }
        }
      }
    }
  ' | awk '!seen[$0]++'
)

if [[ ${#CHANGED_LINES[@]} -eq 0 ]]; then
  echo "Found a diff in $TARGET_FILE, but could not map it to preview lines." >&2
  exit 1
fi

LINES_CSV="$(printf '%s\n' "${CHANGED_LINES[@]}" | paste -sd, -)"

awk \
  -v lines_csv="$LINES_CSV" \
  -v file_label="$TARGET_FILE" \
  -v base_label="$BASE_LABEL" '
  function build_sections(    i) {
    if (heading_count == 0) {
      sec_count = 1
      sec_start[1] = 1
      sec_title[1] = "(whole file)"
    } else {
      if (heading_start[1] > 1) {
        sec_count++
        sec_start[sec_count] = 1
        sec_title[sec_count] = "(preamble)"
      }

      for (i = 1; i <= heading_count; i++) {
        sec_count++
        sec_start[sec_count] = heading_start[i]
        sec_title[sec_count] = heading_title[i]
      }
    }

    for (i = 1; i <= sec_count; i++) {
      if (i < sec_count) {
        sec_end[i] = sec_start[i + 1] - 1
      } else {
        sec_end[i] = total_lines
      }
    }
  }

  function find_section(line_no,    i) {
    for (i = 1; i <= sec_count; i++) {
      if (line_no >= sec_start[i] && line_no <= sec_end[i]) {
        return i
      }
    }

    return sec_count
  }

  function nearest_content_line(line_no,    down, up) {
    if (file_lines[line_no] !~ /^[[:space:]]*$/) {
      return line_no
    }

    for (down = line_no + 1; down <= total_lines; down++) {
      if (file_lines[down] !~ /^[[:space:]]*$/) {
        return down
      }
    }

    for (up = line_no - 1; up >= 1; up--) {
      if (file_lines[up] !~ /^[[:space:]]*$/) {
        return up
      }
    }

    return line_no
  }

  BEGIN {
    split(lines_csv, changed_lines, ",")
  }

  {
    total_lines = NR
    file_lines[NR] = $0

    if ($0 ~ /^(```|~~~)/) {
      in_fence = !in_fence
    }

    if (!in_fence && $0 ~ /^#{1,6}[[:space:]]+/) {
      heading_count++
      heading_start[heading_count] = NR
      heading_title[heading_count] = $0
    }
  }

  END {
    if (total_lines == 0) {
      print "File is empty: " file_label > "/dev/stderr"
      exit 1
    }

    build_sections()

    for (i in changed_lines) {
      line_no = changed_lines[i] + 0
      if (line_no < 1) {
        line_no = 1
      } else if (line_no > total_lines) {
        line_no = total_lines
      }

      line_no = nearest_content_line(line_no)

      section_id = find_section(line_no)
      selected[section_id] = 1
    }

    print "Changed sections in " file_label " relative to " base_label ":"
    print ""

    for (i = 1; i <= sec_count; i++) {
      if (!selected[i]) {
        continue
      }

      printf "===== %s (lines %d-%d) =====\n", sec_title[i], sec_start[i], sec_end[i]
      for (line_no = sec_start[i]; line_no <= sec_end[i]; line_no++) {
        print file_lines[line_no]
      }
      print ""
    }
  }
' "$TARGET_FILE"
