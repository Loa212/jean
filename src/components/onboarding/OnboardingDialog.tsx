/**
 * Onboarding Dialog for CLI Setup
 *
 * Multi-step wizard that handles installation of both Claude CLI and GitHub CLI.
 * Shows on first launch when either CLI is not installed.
 */

import { useState, useEffect, useCallback, useRef } from 'react'
import { CheckCircle2 } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useUIStore } from '@/store/ui-store'
import { useClaudeCliSetup } from '@/services/claude-cli'
import { useGhCliSetup } from '@/services/gh-cli'
import { usePreferences, useSavePreferences } from '@/services/preferences'
import { isWindows } from '@/services/wsl'
import {
  SetupState,
  InstallingState,
  ErrorState,
  WslRequiredState,
} from './CliSetupComponents'
import type { ReleaseInfo } from '@/types/claude-cli'
import type { GhReleaseInfo } from '@/types/gh-cli'

type OnboardingStep =
  | 'claude-setup'
  | 'claude-installing'
  | 'gh-setup'
  | 'gh-installing'
  | 'complete'

interface CliSetupData {
  type: 'claude' | 'gh'
  title: string
  description: string
  versions: (ReleaseInfo | GhReleaseInfo)[]
  isVersionsLoading: boolean
  isInstalling: boolean
  installError: Error | null
  progress: { stage: string; message: string; percent: number } | null
  install: (
    version: string,
    options?: { onSuccess?: () => void; onError?: (error: Error) => void }
  ) => void
  currentVersion: string | null | undefined
}

/**
 * Wrapper that only renders content when open.
 * Prevents duplicate event listeners when dialog is closed.
 */
export function OnboardingDialog() {
  const onboardingOpen = useUIStore(state => state.onboardingOpen)

  if (!onboardingOpen) {
    return null
  }

  return <OnboardingDialogContent />
}

/**
 * Inner component with all hook logic.
 * Only mounted when dialog is actually open.
 */
function OnboardingDialogContent() {
  const {
    onboardingOpen,
    setOnboardingOpen,
    onboardingStartStep,
    setOnboardingStartStep,
  } = useUIStore()

  const claudeSetup = useClaudeCliSetup()
  const ghSetup = useGhCliSetup()

  const [step, setStep] = useState<OnboardingStep>('claude-setup')
  const [claudeVersion, setClaudeVersion] = useState<string | null>(null)
  const [ghVersion, setGhVersion] = useState<string | null>(null)
  const [claudeInstallFailed, setClaudeInstallFailed] = useState(false)
  const [ghInstallFailed, setGhInstallFailed] = useState(false)
  // Track when step was deliberately set via onboardingStartStep to prevent auto-skip
  const deliberateStepRef = useRef(false)

  // Filter to stable releases only
  const stableClaudeVersions = claudeSetup.versions.filter(v => !v.prerelease)
  const stableGhVersions = ghSetup.versions.filter(v => !v.prerelease)

  // Auto-select latest versions when loaded
  useEffect(() => {
    if (!claudeVersion && stableClaudeVersions.length > 0) {
      queueMicrotask(() =>
        setClaudeVersion(stableClaudeVersions[0]?.version ?? null)
      )
    }
  }, [claudeVersion, stableClaudeVersions])

  useEffect(() => {
    if (!ghVersion && stableGhVersions.length > 0) {
      queueMicrotask(() => setGhVersion(stableGhVersions[0]?.version ?? null))
    }
  }, [ghVersion, stableGhVersions])

  // Determine initial step when dialog opens
  useEffect(() => {
    if (!onboardingOpen) {
      // Reset ref when dialog closes
      deliberateStepRef.current = false
      return
    }

    // Reset error states on open
    queueMicrotask(() => {
      setClaudeInstallFailed(false)
      setGhInstallFailed(false)
    })

    // If a specific start step was requested (from Settings)
    if (onboardingStartStep === 'gh') {
      deliberateStepRef.current = true
      queueMicrotask(() => {
        setStep('gh-setup')
        setOnboardingStartStep(null)
      })
      return
    }

    if (onboardingStartStep === 'claude') {
      deliberateStepRef.current = true
      queueMicrotask(() => {
        setStep('claude-setup')
        setOnboardingStartStep(null)
      })
      return
    }

    // Skip auto-skip logic if step was deliberately set
    if (deliberateStepRef.current) {
      return
    }

    // Auto-skip based on installation status (only for fresh opens)
    if (claudeSetup.status?.installed && ghSetup.status?.installed) {
      queueMicrotask(() => setStep('complete'))
    } else if (claudeSetup.status?.installed) {
      queueMicrotask(() => setStep('gh-setup'))
    } else {
      queueMicrotask(() => setStep('claude-setup'))
    }
  }, [
    onboardingOpen,
    onboardingStartStep,
    claudeSetup.status?.installed,
    ghSetup.status?.installed,
    setOnboardingStartStep,
  ])

  const handleClaudeInstall = useCallback(() => {
    if (!claudeVersion) return
    setStep('claude-installing')
    claudeSetup.install(claudeVersion, {
      onSuccess: () => {
        // Move to gh setup
        if (ghSetup.status?.installed) {
          setStep('complete')
        } else {
          setStep('gh-setup')
        }
      },
      onError: () => {
        setClaudeInstallFailed(true)
        setStep('claude-setup')
      },
    })
  }, [claudeVersion, claudeSetup, ghSetup.status?.installed])

  const handleGhInstall = useCallback(() => {
    if (!ghVersion) return
    setStep('gh-installing')
    ghSetup.install(ghVersion, {
      onSuccess: () => {
        setStep('complete')
      },
      onError: () => {
        setGhInstallFailed(true)
        setStep('gh-setup')
      },
    })
  }, [ghVersion, ghSetup])

  const handleComplete = useCallback(() => {
    claudeSetup.refetchStatus()
    ghSetup.refetchStatus()
    setOnboardingOpen(false)
    setOnboardingStartStep(null)
  }, [claudeSetup, ghSetup, setOnboardingOpen, setOnboardingStartStep])

  const handleSkipGh = useCallback(() => {
    // Only available on error - graceful fallback
    setStep('complete')
  }, [])

  // Build CLI setup data based on current step
  const getCliSetupData = (): CliSetupData | null => {
    if (step === 'claude-setup' || step === 'claude-installing') {
      return {
        type: 'claude',
        title: 'Claude CLI',
        description: 'Claude CLI is required for AI chat functionality.',
        versions: stableClaudeVersions,
        isVersionsLoading: claudeSetup.isVersionsLoading,
        isInstalling: claudeSetup.isInstalling,
        installError: claudeInstallFailed ? claudeSetup.installError : null,
        progress: claudeSetup.progress,
        install: claudeSetup.install,
        currentVersion: claudeSetup.status?.version,
      }
    }

    if (step === 'gh-setup' || step === 'gh-installing') {
      return {
        type: 'gh',
        title: 'GitHub CLI',
        description: 'GitHub CLI is required for GitHub integration.',
        versions: stableGhVersions,
        isVersionsLoading: ghSetup.isVersionsLoading,
        isInstalling: ghSetup.isInstalling,
        installError: ghInstallFailed ? ghSetup.installError : null,
        progress: ghSetup.progress,
        install: ghSetup.install,
        currentVersion: ghSetup.status?.version,
      }
    }

    return null
  }

  const cliData = getCliSetupData()

  // Determine if we're in reinstall mode (CLI already installed but user wants to change version)
  const isClaudeReinstall =
    claudeSetup.status?.installed && step === 'claude-setup'
  const isGhReinstall = ghSetup.status?.installed && step === 'gh-setup'

  // Determine dialog title and description
  const getDialogContent = () => {
    if (step === 'complete') {
      return {
        title: 'Setup Complete',
        description: 'All required tools have been installed.',
        showClose: true,
      }
    }

    if (step === 'claude-setup' || step === 'claude-installing') {
      return {
        title: isClaudeReinstall
          ? 'Change Claude CLI Version'
          : 'Welcome to Jean',
        description: isClaudeReinstall
          ? 'Select a version to install. This will replace the current installation.'
          : 'Jean needs Claude CLI to work. Please install it to continue.',
        showClose: isClaudeReinstall,
      }
    }

    if (step === 'gh-setup' || step === 'gh-installing') {
      return {
        title: isGhReinstall
          ? 'Change GitHub CLI Version'
          : 'Install GitHub CLI',
        description: isGhReinstall
          ? 'Select a version to install. This will replace the current installation.'
          : 'GitHub CLI is required for GitHub integration.',
        showClose: isGhReinstall,
      }
    }

    return { title: 'Setup', description: '', showClose: false }
  }

  const dialogContent = getDialogContent()

  // Step indicator
  const renderStepIndicator = () => {
    const claudeComplete =
      claudeSetup.status?.installed ||
      step === 'gh-setup' ||
      step === 'gh-installing' ||
      step === 'complete'
    const ghComplete = ghSetup.status?.installed || step === 'complete'
    const isClaudeStep = step === 'claude-setup' || step === 'claude-installing'
    const isGhStep = step === 'gh-setup' || step === 'gh-installing'

    return (
      <div className="flex items-center justify-center gap-2 mb-4">
        <div
          className={`flex items-center gap-1.5 px-2 py-1 rounded text-xs ${
            isClaudeStep
              ? 'bg-primary text-primary-foreground'
              : claudeComplete
                ? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
                : 'bg-muted text-muted-foreground'
          }`}
        >
          {claudeComplete && !isClaudeStep ? (
            <CheckCircle2 className="size-3" />
          ) : (
            <span className="font-medium">1</span>
          )}
          <span>Claude CLI</span>
        </div>
        <div className="w-4 h-px bg-border" />
        <div
          className={`flex items-center gap-1.5 px-2 py-1 rounded text-xs ${
            isGhStep
              ? 'bg-primary text-primary-foreground'
              : ghComplete
                ? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
                : 'bg-muted text-muted-foreground'
          }`}
        >
          {ghComplete && !isGhStep ? (
            <CheckCircle2 className="size-3" />
          ) : (
            <span className="font-medium">2</span>
          )}
          <span>GitHub CLI</span>
        </div>
        <div className="w-4 h-px bg-border" />
        <div
          className={`flex items-center gap-1.5 px-2 py-1 rounded text-xs ${
            step === 'complete'
              ? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
              : 'bg-muted text-muted-foreground'
          }`}
        >
          {step === 'complete' ? (
            <CheckCircle2 className="size-3" />
          ) : (
            <span className="font-medium">3</span>
          )}
          <span>Done</span>
        </div>
      </div>
    )
  }

  return (
    <Dialog open={onboardingOpen} onOpenChange={setOnboardingOpen}>
      <DialogContent
        className="sm:max-w-[500px]"
        showCloseButton={dialogContent.showClose}
        preventClose={!dialogContent.showClose}
      >
        <DialogHeader>
          <DialogTitle className="text-xl">{dialogContent.title}</DialogTitle>
          <DialogDescription>{dialogContent.description}</DialogDescription>
        </DialogHeader>

        <div className="py-4">
          {renderStepIndicator()}

          {step === 'complete' ? (
            <SuccessState
              claudeVersion={claudeSetup.status?.version}
              ghVersion={ghSetup.status?.version}
              onContinue={handleComplete}
            />
          ) : step === 'claude-installing' && cliData ? (
            <InstallingState cliName="Claude CLI" progress={cliData.progress} />
          ) : step === 'gh-installing' && cliData ? (
            <InstallingState cliName="GitHub CLI" progress={cliData.progress} />
          ) : step === 'claude-setup' && cliData ? (
            claudeInstallFailed && cliData.installError ? (
              <ErrorState
                cliName="Claude CLI"
                error={cliData.installError}
                onRetry={handleClaudeInstall}
              />
            ) : (
              <SetupState
                cliName="Claude CLI"
                versions={stableClaudeVersions}
                selectedVersion={claudeVersion}
                currentVersion={
                  isClaudeReinstall ? cliData.currentVersion : null
                }
                isLoading={cliData.isVersionsLoading}
                onVersionChange={setClaudeVersion}
                onInstall={handleClaudeInstall}
              />
            )
          ) : step === 'gh-setup' && cliData ? (
            ghInstallFailed && cliData.installError ? (
              <ErrorState
                cliName="GitHub CLI"
                error={cliData.installError}
                onRetry={handleGhInstall}
                onSkip={handleSkipGh}
              />
            ) : (
              <SetupState
                cliName="GitHub CLI"
                versions={stableGhVersions}
                selectedVersion={ghVersion}
                currentVersion={isGhReinstall ? cliData.currentVersion : null}
                isLoading={cliData.isVersionsLoading}
                onVersionChange={setGhVersion}
                onInstall={handleGhInstall}
              />
            )
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}

interface SuccessStateProps {
  claudeVersion: string | null | undefined
  ghVersion: string | null | undefined
  onContinue: () => void
}

function SuccessState({
  claudeVersion,
  ghVersion,
  onContinue,
}: SuccessStateProps) {
  return (
    <div className="space-y-6">
      <div className="flex flex-col items-center gap-4">
        <CheckCircle2 className="size-10 text-green-500" />
        <div className="text-center">
          <p className="font-medium">All Tools Installed</p>
          <div className="text-sm text-muted-foreground mt-2 space-y-1">
            {claudeVersion && <p>Claude CLI: v{claudeVersion}</p>}
            {ghVersion && <p>GitHub CLI: v{ghVersion}</p>}
            {!claudeVersion && !ghVersion && <p>Installation complete</p>}
          </div>
        </div>
      </div>

      <Button onClick={onContinue} className="w-full" size="lg">
        Continue to Jean
      </Button>
    </div>
  )
}

export default OnboardingDialog
