import { useEffect, useRef, useState } from 'react'
import { ExternalLink, GitMerge, GitPullRequest } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Checkbox } from '@/components/ui/checkbox'
import { useChatStore } from '@/store/chat-store'
import { openExternal } from '@/lib/platform'
import { ChangedFilesSummary } from './ChangedFilesSummary'

/** localStorage key for suppressing merge confirmation */
const SKIP_MERGE_CONFIRM_KEY = 'jean:skip-merge-confirm'

interface IssueActionEndStateProps {
  worktreeId: string
  hasOpenPr: boolean
  prUrl: string | null | undefined
  isSending: boolean
  issueAction: 'implement' | 'ship' | undefined
  onOpenPr: () => Promise<void>
  onMergePr: () => Promise<unknown>
}

/**
 * End-state UI for Implement/Ship issue actions.
 * Shows changed files + action buttons at the bottom of chat when the session finishes.
 *
 * - Implement mode: "Create PR" button → after PR created → "View PR" + "Merge PR"
 * - Ship mode: auto-creates PR when session finishes → "View PR" + "Merge PR"
 */
export function IssueActionEndState({
  worktreeId,
  hasOpenPr,
  prUrl,
  isSending,
  issueAction: issueActionProp,
  onOpenPr,
  onMergePr,
}: IssueActionEndStateProps) {
  // Zustand for instant update during current session; prop (TanStack Query) for persistence after reload
  const zustandIssueAction = useChatStore(
    state => state.worktreeIssueActions[worktreeId]
  )
  const issueAction = zustandIssueAction ?? issueActionProp
  const isLoading = useChatStore(
    state => state.worktreeLoadingOperations[worktreeId]
  )
  const autoShipTriggered = useRef(false)
  const [showMergeConfirm, setShowMergeConfirm] = useState(false)
  const [skipConfirmChecked, setSkipConfirmChecked] = useState(false)

  // Ship mode: auto-create PR when session goes idle
  useEffect(() => {
    if (
      issueAction === 'ship' &&
      !isSending &&
      !hasOpenPr &&
      !isLoading &&
      !autoShipTriggered.current
    ) {
      autoShipTriggered.current = true
      onOpenPr()
    }
  }, [issueAction, isSending, hasOpenPr, isLoading, onOpenPr])

  // Don't render if this session wasn't started via implement/ship
  if (!issueAction) return null

  // Don't render while Claude is still working
  if (isSending) return null

  const handleMergeClick = () => {
    const skipConfirm = localStorage.getItem(SKIP_MERGE_CONFIRM_KEY) === 'true'
    if (skipConfirm) {
      onMergePr()
    } else {
      setShowMergeConfirm(true)
    }
  }

  const handleMergeConfirm = () => {
    if (skipConfirmChecked) {
      localStorage.setItem(SKIP_MERGE_CONFIRM_KEY, 'true')
    }
    setShowMergeConfirm(false)
    onMergePr()
  }

  return (
    <>
      <div className="mx-auto w-full max-w-3xl px-4 py-3">
        <div className="rounded-lg border border-border bg-card p-4 space-y-3">
          <ChangedFilesSummary worktreeId={worktreeId} />

          <div className="flex items-center gap-2 pt-1">
            {!hasOpenPr ? (
              <Button
                size="sm"
                onClick={() => onOpenPr()}
                disabled={!!isLoading}
              >
                <GitPullRequest className="h-4 w-4 mr-1.5" />
                {isLoading === 'pr' ? 'Creating PR...' : 'Create PR'}
              </Button>
            ) : (
              <>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => prUrl && openExternal(prUrl)}
                >
                  <ExternalLink className="h-4 w-4 mr-1.5" />
                  View PR
                </Button>
                <Button
                  size="sm"
                  variant="default"
                  onClick={handleMergeClick}
                  disabled={!!isLoading}
                >
                  <GitMerge className="h-4 w-4 mr-1.5" />
                  {isLoading === 'merge-pr' ? 'Merging...' : 'Merge PR'}
                </Button>
              </>
            )}
          </div>
        </div>
      </div>

      <AlertDialog open={showMergeConfirm} onOpenChange={setShowMergeConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Merge this PR?</AlertDialogTitle>
            <AlertDialogDescription>
              This will squash-merge the PR on GitHub and archive the worktree.
              This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="flex items-center gap-2 px-1">
            <Checkbox
              id="skip-merge-confirm"
              checked={skipConfirmChecked}
              onCheckedChange={checked =>
                setSkipConfirmChecked(checked === true)
              }
            />
            <label
              htmlFor="skip-merge-confirm"
              className="text-sm text-muted-foreground cursor-pointer"
            >
              Don't ask again
            </label>
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleMergeConfirm}>
              Merge
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
