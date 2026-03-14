import { useEffect } from 'react'
import { useTheme } from '#/stores/theme'

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const theme = useTheme((state) => state.theme)

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme)
  }, [theme])

  return <>{children}</>
}
