set -euo pipefail
VERSION="$1"

if [ -z "$VERSION" ]; then
  echo "Usage ./release.sh <version>"
  exit 1
fi

if ! git diff --quiet; then
  echo "Error: There are unstaged changes in the repository."
  exit 1
fi

cargo install cargo-edit

cd svgrtypes && cargo set-version "$VERSION" && cargo publish;
cd ../usvgr && cargo set-version "$VERSION" && cargo publish;
cd ../usvgr-text-layout && cargo set-version "$VERSION" && cargo publish;
cd .. && cargo set-version "$VERSION" && cargo publish;

git add --all 
git commit -m "chore(release): v$VERSION"
git tag "v$VERSION"
git push origin "v$VERSION"
git push

