import type { ReactNode } from "react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
export function Modal({ title, icon, children, onClose }: { title: string; icon?: ReactNode; children: ReactNode; onClose: () => void }) {
  return <Dialog open onOpenChange={open => { if (!open) onClose(); }}><DialogContent className="max-h-[85vh] overflow-auto sm:max-w-xl"><DialogHeader><DialogTitle className="flex items-center gap-2 pr-8 text-sm font-semibold [&_svg]:size-[1em]">{icon}{title}</DialogTitle></DialogHeader>{children}</DialogContent></Dialog>;
}
