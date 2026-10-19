import {createContext} from "react";

export const StatusBarSlot = createContext<((element: HTMLDivElement | null) => void) | undefined>(undefined);
