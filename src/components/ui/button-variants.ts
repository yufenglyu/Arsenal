import { cva } from "class-variance-authority"

// One geometry for actions; size aliases remain compatible with existing callers.
export const buttonGeometry = "h-8 gap-2 rounded-md px-3 py-0 text-sm leading-5 font-normal [&_svg]:size-[1em]"
export const statusButtonGeometry = "h-8 gap-2 rounded-md px-3 py-0 text-xs leading-4 font-normal [&_svg]:size-[1em]"
export const iconButtonGeometry = "size-8 gap-0 rounded-md p-0 text-sm leading-5 font-normal [&_svg]:size-[1em]"

const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center border border-transparent bg-clip-padding whitespace-nowrap transition-colors outline-none select-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-2 aria-invalid:ring-destructive/20 [&_svg]:pointer-events-none [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/90",
        outline: "border-border bg-transparent text-foreground hover:bg-muted aria-expanded:bg-muted",
        secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/80 aria-expanded:bg-secondary/80",
        ghost: "bg-transparent hover:bg-muted hover:text-foreground aria-expanded:bg-muted",
        destructive: "border-destructive/30 bg-transparent text-destructive hover:bg-destructive/10 focus-visible:border-destructive focus-visible:ring-destructive/30",
        link: "bg-transparent text-primary hover:bg-primary/10",
      },
      size: {
        default: buttonGeometry,
        status: statusButtonGeometry,
        xs: buttonGeometry,
        sm: buttonGeometry,
        lg: buttonGeometry,
        icon: iconButtonGeometry,
        "icon-xs": iconButtonGeometry,
        "icon-sm": iconButtonGeometry,
        "icon-lg": iconButtonGeometry,
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  }
)

export { buttonVariants }
