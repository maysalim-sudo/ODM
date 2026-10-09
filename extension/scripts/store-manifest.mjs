// The Chrome Web Store and Edge Add-ons assign their own extension ID and reject a manifest "key".
export function storeManifest(manifest) {
  const { key: _key, ...rest } = manifest;
  return rest;
}
