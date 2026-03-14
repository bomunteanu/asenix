import { Outlet, createRootRoute } from '@tanstack/react-router'
import Layout from '../components/Layout'

import '../styles.css'

export const Route = createRootRoute({
  component: RootComponent,
})

function RootComponent() {
  return (
    <Layout>
      <Outlet />
    </Layout>
  )
}
