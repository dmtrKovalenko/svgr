set -euo pipefail
VERSION="$1"

if ! git diff --quiet; then
  echo "Error: There are unstaged changes in the repository."
  exit 1
fi
cargo install cargo-edit
cargo set-version "$VERSION"

git add --all 
git commit -m "chore(release): v$VERSION"

cd crates/svgrtypes && cargo publish;
cd ../usvgr  && cargo publish;
cd ../svgr && cargo publish;

git tag "v$VERSION"

git push origin "v$VERSION"

git push

