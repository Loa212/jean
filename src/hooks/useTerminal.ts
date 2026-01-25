import { useEffect, useRef, useCallback } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useTerminalStore } from '@/store/terminal-store'
import type {
  TerminalOutputEvent,
  TerminalStartedEvent,
  TerminalStoppedEvent,
} from '@/types/terminal'

interface UseTerminalOptions {
  terminalId: string
  worktreePath: string
  command?: string | null
  /** Override the use_wsl preference. If undefined, uses the stored preference. */
  useWslOverride?: boolean
}

export function useTerminal({ terminalId, worktreePath, command, useWslOverride }: UseTerminalOptions) {
  const terminalRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const unlistenersRef = useRef<UnlistenFn[]>([])

  const { setTerminalRunning } = useTerminalStore.getState()

  const initTerminal = useCallback(
    async (container: HTMLDivElement) => {
      console.log('[useTerminal] initTerminal called, terminalId:', terminalId, 'existing terminal:', !!terminalRef.current)

      if (terminalRef.current) {
        console.log('[useTerminal] Terminal already exists, returning early')
        return
      }

      containerRef.current = container
      console.log('[useTerminal] Creating new Terminal instance')

      const terminal = new Terminal({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Monaco, Consolas, monospace',
        theme: {
          background: '#1a1a1a',
          foreground: '#e5e5e5',
          cursor: '#e5e5e5',
          selectionBackground: '#404040',
        },
        allowProposedApi: true,
      })

      const fitAddon = new FitAddon()
      terminal.loadAddon(fitAddon)

      console.log('[useTerminal] Opening terminal in container')
      terminal.open(container)
      console.log('[useTerminal] Terminal opened, element:', terminal.element)

      terminalRef.current = terminal
      fitAddonRef.current = fitAddon

      // Handle user input
      terminal.onData(data => {
        invoke('terminal_write', { terminalId, data }).catch(console.error)
      })

      // Listen for terminal output
      console.log('[useTerminal] Setting up event listeners for:', terminalId)
      const unlistenOutput = await listen<TerminalOutputEvent>('terminal:output', event => {
        if (event.payload.terminal_id === terminalId) {
          console.log('[useTerminal] Received output for', terminalId, '- length:', event.payload.data.length)
          terminal.write(event.payload.data)
        }
      })

      const unlistenStarted = await listen<TerminalStartedEvent>('terminal:started', event => {
        console.log('[useTerminal] Received terminal:started event:', event.payload.terminal_id)
        if (event.payload.terminal_id === terminalId) {
          console.log('[useTerminal] Terminal started:', terminalId)
          setTerminalRunning(terminalId, true)
        }
      })

      const unlistenStopped = await listen<TerminalStoppedEvent>('terminal:stopped', event => {
        console.log('[useTerminal] Received terminal:stopped event:', event.payload.terminal_id)
        if (event.payload.terminal_id === terminalId) {
          setTerminalRunning(terminalId, false)
          const exitCode = event.payload.exit_code
          terminal.writeln(`\r\n\x1b[90m[Process exited with code ${exitCode ?? 'unknown'}]\x1b[0m`)
        }
      })
      console.log('[useTerminal] Event listeners set up')

      unlistenersRef.current = [unlistenOutput, unlistenStarted, unlistenStopped]

      // Delay initial fit and start to ensure container dimensions are settled
      requestAnimationFrame(() => {
        console.log('[useTerminal] RAF executing, fitting terminal')
        fitAddon.fit()
        const { cols, rows } = terminal
        console.log('[useTerminal] Invoking start_terminal:', { terminalId, worktreePath, cols, rows, command, useWslOverride })
        invoke('start_terminal', {
          terminalId,
          worktreePath,
          cols,
          rows,
          command: command ?? null,
          useWslOverride: useWslOverride ?? null,
        }).then(() => {
          console.log('[useTerminal] start_terminal invoke succeeded')
        }).catch(error => {
          console.error('[useTerminal] start_terminal invoke failed:', error)
          terminal.writeln(`\x1b[31mFailed to start terminal: ${error}\x1b[0m`)
        })
        // Focus the terminal so user can start typing immediately
        terminal.focus()
        console.log('[useTerminal] Terminal focused')
      })
    },
    [terminalId, worktreePath, command, useWslOverride, setTerminalRunning]
  )

  const fit = useCallback(() => {
    if (fitAddonRef.current && terminalRef.current) {
      fitAddonRef.current.fit()

      const { cols, rows } = terminalRef.current
      invoke('terminal_resize', { terminalId, cols, rows }).catch(console.error)
    }
  }, [terminalId])

  const focus = useCallback(() => {
    if (terminalRef.current) {
      terminalRef.current.focus()
    }
  }, [])

  const dispose = useCallback(async () => {
    // Stop terminal on backend
    try {
      await invoke('stop_terminal', { terminalId })
    } catch {
      // Ignore errors
    }

    // Cleanup listeners
    for (const unlisten of unlistenersRef.current) {
      unlisten()
    }
    unlistenersRef.current = []

    // Dispose terminal
    if (terminalRef.current) {
      terminalRef.current.dispose()
      terminalRef.current = null
    }
    fitAddonRef.current = null
    containerRef.current = null
  }, [terminalId])

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      dispose()
    }
  }, [dispose])

  return {
    initTerminal,
    fit,
    focus,
    dispose,
    terminalRef,
  }
}
