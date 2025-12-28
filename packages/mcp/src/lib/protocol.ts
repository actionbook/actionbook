export const SUPPORTED_PROTOCOL_VERSIONS = ["2025-03-26", "2024-11-05"] as const;

export const CURRENT_PROTOCOL_VERSION = SUPPORTED_PROTOCOL_VERSIONS[0];

export function isSupportedVersion(version: string): boolean {
  return SUPPORTED_PROTOCOL_VERSIONS.includes(version as (typeof SUPPORTED_PROTOCOL_VERSIONS)[number]);
}

export function negotiateVersion(clientVersion: string): string {
  if (isSupportedVersion(clientVersion)) {
    return clientVersion;
  }
  return CURRENT_PROTOCOL_VERSION;
}
