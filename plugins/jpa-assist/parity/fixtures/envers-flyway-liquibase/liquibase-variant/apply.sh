#!/bin/sh
# Usage: liquibase-variant/apply.sh [includeall]
# Swaps Flyway for Liquibase in the parent project. Run from anywhere; undo with: git checkout -- . && git clean -fd
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
cp "$here/pom.xml" "$root/pom.xml"
cp "$here/overlay/application.properties" "$root/src/main/resources/application.properties"
rm -rf "$root/src/main/resources/db/migration" "$root/src/main/java/db/migration"
mkdir -p "$root/src/main/resources/db"
if [ "$1" = "includeall" ]; then
  cp -R "$here/overlay/resources/db/changelog-includeall" "$root/src/main/resources/db/changelog"
else
  cp -R "$here/overlay/resources/db/changelog" "$root/src/main/resources/db/changelog"
fi
echo "Liquibase overlay applied ($root)"
