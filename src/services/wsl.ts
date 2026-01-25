import { invoke } from '@tauri-apps/api/core'
import { useQuery } from '@tanstack/react-query'

/**
 * Check if the current platform is Windows
 */
export const isWindows = () => navigator.platform?.includes('Win') ?? false

/**
 * Check if a path is a WSL UNC path (\\wsl.localhost\... or \\wsl$\...)
 */
export const isWslUncPath = (path: string): boolean => {
  return path.startsWith('\\\\wsl.localhost\\') || path.startsWith('\\\\wsl$\\')
}

/**
 * Query key for WSL availability
 */
export const wslQueryKeys = {
  available: ['wsl-available'] as const,
}

/**
 * Hook to check if WSL is available on Windows
 *
 * Returns:
 * - true: WSL is installed and available
 * - false: WSL is not available (or not on Windows)
 *
 * Only queries on Windows, returns false immediately on other platforms.
 */
export function useWslAvailable() {
  return useQuery({
    queryKey: wslQueryKeys.available,
    queryFn: async () => {
      if (!isWindows()) {
        return false
      }
      return invoke<boolean>('check_wsl_available')
    },
    staleTime: Infinity, // WSL availability doesn't change during app session
    enabled: isWindows(),
  })
}
