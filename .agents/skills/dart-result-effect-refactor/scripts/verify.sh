#!/usr/bin/env bash
set -euo pipefail

if [[ ! -f pubspec.yaml ]]; then
  echo "error: pubspec.yaml not found; run this script from the Dart/Flutter project root." >&2
  exit 1
fi

echo "==> dart format ."
dart format .

if grep -Eq '^[[:space:]]*flutter:[[:space:]]*$' pubspec.yaml; then
  echo "==> flutter analyze"
  flutter analyze

  echo "==> flutter test"
  flutter test
else
  echo "==> dart analyze"
  dart analyze

  echo "==> dart test"
  dart test
fi
