import { useQuery } from '@tanstack/react-query'
import { FileText } from 'lucide-react'
import { invoke } from '@/lib/transport'
import type { ChangedFileStat } from '@/types/projects'
import { cn } from '@/lib/utils'

interface ChangedFilesSummaryProps {
  worktreeId: string
  className?: string
}

export function ChangedFilesSummary({
  worktreeId,
  className,
}: ChangedFilesSummaryProps) {
  const { data: files, isLoading } = useQuery({
    queryKey: ['changed-files', worktreeId],
    queryFn: () =>
      invoke<ChangedFileStat[]>('get_changed_files', { worktreeId }),
    staleTime: 30_000,
  })

  if (isLoading) {
    return (
      <div className={cn('text-xs text-muted-foreground py-2', className)}>
        Loading changed files...
      </div>
    )
  }

  if (!files || files.length === 0) {
    return (
      <div className={cn('text-xs text-muted-foreground py-2', className)}>
        No changed files
      </div>
    )
  }

  const totalAdded = files.reduce((sum, f) => sum + f.additions, 0)
  const totalRemoved = files.reduce((sum, f) => sum + f.deletions, 0)

  return (
    <div className={cn('text-xs', className)}>
      <div className="flex items-center gap-2 mb-1.5 text-muted-foreground">
        <FileText className="h-3.5 w-3.5" />
        <span>
          {files.length} file{files.length !== 1 ? 's' : ''} changed
        </span>
        <span className="text-green-600 dark:text-green-400">
          +{totalAdded}
        </span>
        <span className="text-red-600 dark:text-red-400">-{totalRemoved}</span>
      </div>
      <div className="max-h-40 overflow-y-auto space-y-0.5">
        {files.map(file => (
          <div
            key={file.file}
            className="flex items-center justify-between px-2 py-0.5 rounded hover:bg-muted/50"
          >
            <span className="truncate text-foreground/80">{file.file}</span>
            <span className="flex gap-2 shrink-0 ml-2">
              {file.additions > 0 && (
                <span className="text-green-600 dark:text-green-400">
                  +{file.additions}
                </span>
              )}
              {file.deletions > 0 && (
                <span className="text-red-600 dark:text-red-400">
                  -{file.deletions}
                </span>
              )}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}
