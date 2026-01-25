/**
 * Types for GitHub CLI integration
 */

/**
 * Status of the GitHub CLI installation
 */
export interface GhCliStatus {
  /** Whether GitHub CLI is installed */
  installed: boolean
  /** Installed version (if any) */
  version: string | null
  /** Path to the CLI binary (if installed) */
  path: string | null
  /** Mode of operation ("wsl" or "native") */
  mode: string
}

/**
 * Result of checking GitHub CLI authentication status
 */
export interface GhAuthStatus {
  /** Whether the CLI is authenticated */
  authenticated: boolean
  /** Error message if authentication check failed */
  error: string | null
}

/**
 * Information about a GitHub CLI release
 */
export interface GhReleaseInfo {
  /** Version string (e.g., "2.40.0") */
  version: string
  /** Git tag name (e.g., "v2.40.0") - camelCase alias */
  tagName: string
  /** Publication date in ISO format - camelCase alias */
  publishedAt: string
  /** Whether this is a prerelease */
  prerelease: boolean
}

/**
 * Progress event for CLI installation
 */
export interface GhInstallProgress {
  /** Current stage of installation */
  stage:
    | 'starting'
    | 'downloading'
    | 'extracting'
    | 'installing'
    | 'verifying'
    | 'complete'
  /** Progress message */
  message: string
  /** Percentage complete (0-100) */
  percent: number
}
