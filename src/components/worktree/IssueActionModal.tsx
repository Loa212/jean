import { useCallback, useState, useRef } from 'react'
import { Search, FileText, Code, Rocket } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { cn } from '@/lib/utils'

type IssueAction = 'investigate' | 'plan' | 'implement' | 'ship'

const OPTIONS: {
  id: IssueAction
  label: string
  description: string
  key: string
  icon: typeof Search
}[] = [
  {
    id: 'investigate',
    label: 'Investigate',
    description: 'Explore the issue without making changes',
    key: 'I',
    icon: Search,
  },
  {
    id: 'plan',
    label: 'Plan',
    description: 'Create a plan for the issue',
    key: 'P',
    icon: FileText,
  },
  {
    id: 'implement',
    label: 'Implement',
    description: 'Implement a fix, then create a PR manually',
    key: 'M',
    icon: Code,
  },
  {
    id: 'ship',
    label: 'Ship',
    description: 'Implement, auto-create PR, then merge',
    key: 'S',
    icon: Rocket,
  },
]

const KEY_MAP: Record<string, IssueAction> = {
  i: 'investigate',
  p: 'plan',
  m: 'implement',
  s: 'ship',
}

interface IssueActionModalProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  issueNumber: number
  issueTitle: string
  onSelect: (action: IssueAction) => void
}

export function IssueActionModal({
  open,
  onOpenChange,
  issueNumber,
  issueTitle,
  onSelect,
}: IssueActionModalProps) {
  const contentRef = useRef<HTMLDivElement>(null)
  const [selectedOption, setSelectedOption] = useState<IssueAction>('investigate')

  const handleSelect = useCallback(
    (action: IssueAction) => {
      onSelect(action)
      onOpenChange(false)
    },
    [onSelect, onOpenChange]
  )

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const key = e.key.toLowerCase()

      const mapped = KEY_MAP[key]
      if (mapped) {
        e.preventDefault()
        handleSelect(mapped)
        return
      }

      if (key === 'enter') {
        e.preventDefault()
        handleSelect(selectedOption)
      } else if (key === 'arrowdown' || key === 'arrowup') {
        e.preventDefault()
        const currentIndex = OPTIONS.findIndex(o => o.id === selectedOption)
        const newIndex =
          key === 'arrowdown'
            ? (currentIndex + 1) % OPTIONS.length
            : (currentIndex - 1 + OPTIONS.length) % OPTIONS.length
        const newOption = OPTIONS[newIndex]
        if (newOption) {
          setSelectedOption(newOption.id)
        }
      }
    },
    [handleSelect, selectedOption]
  )

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        ref={contentRef}
        tabIndex={-1}
        className="sm:max-w-[400px] p-0 outline-none"
        onOpenAutoFocus={e => {
          e.preventDefault()
          contentRef.current?.focus()
        }}
        onKeyDown={handleKeyDown}
      >
        <DialogHeader className="px-4 pt-5 pb-2">
          <DialogTitle className="text-sm font-medium">
            <span className="text-muted-foreground">#{issueNumber}</span>{' '}
            <span className="line-clamp-1">{issueTitle}</span>
          </DialogTitle>
        </DialogHeader>

        <div className="pb-2">
          {OPTIONS.map(option => {
            const Icon = option.icon
            const isSelected = selectedOption === option.id

            return (
              <button
                key={option.id}
                onClick={() => handleSelect(option.id)}
                onMouseEnter={() => setSelectedOption(option.id)}
                className={cn(
                  'w-full flex items-center justify-between px-4 py-2.5 text-sm transition-colors',
                  'focus:outline-none hover:bg-accent',
                  isSelected && 'bg-accent'
                )}
              >
                <div className="flex items-center gap-3">
                  <Icon className="h-4 w-4 text-muted-foreground" />
                  <div className="text-left">
                    <div>{option.label}</div>
                    <div className="text-xs text-muted-foreground">
                      {option.description}
                    </div>
                  </div>
                </div>
                <kbd className="text-xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
                  {option.key}
                </kbd>
              </button>
            )
          })}
        </div>
      </DialogContent>
    </Dialog>
  )
}
