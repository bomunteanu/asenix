import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { Project } from '#/lib/bindings'

interface ActiveProjectStore {
  activeProject: Project | null
  setActiveProject: (project: Project | null) => void
}

export const useActiveProject = create<ActiveProjectStore>()(
  persist(
    (set) => ({
      activeProject: null,
      setActiveProject: (project) => set({ activeProject: project }),
    }),
    {
      name: 'active-project-storage',
    }
  )
)
