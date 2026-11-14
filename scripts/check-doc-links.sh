#!/usr/bin/env bash
# Re-check the official owner-manual links in crates/core/src/providers/oem_docs.rs.
# Any link that doesn't answer 200 should be fixed or removed, and CHECKED_ON updated.
set -u
fail=0
while read -r url; do
  code=$(curl -s -o /dev/null -L --max-time 20 -A "Mozilla/5.0 AutoManagerLinkCheck" -w "%{http_code}" "$url")
  echo "$code $url"
  [ "$code" = "200" ] || fail=1
done < <(grep -oE '"https://[^"]+"' "$(dirname "$0")/../crates/core/src/providers/oem_docs.rs" | tr -d '"')
exit $fail
