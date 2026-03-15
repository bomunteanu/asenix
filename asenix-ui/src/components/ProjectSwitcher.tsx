import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { ChevronDown, FolderOpen, Plus, X, Pencil, Trash2, Check } from 'lucide-react'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { useActiveProject } from '#/stores/active-project'
import type { Project } from '#/lib/bindings'

const inputCls = 'w-full p-1.5 border border-[var(--border)] rounded bg-[var(--bg)] text-[var(--text-primary)] placeholder-[var(--text-muted)] text-sm focus:outline-none focus:border-[var(--accent)]'

// ── Create modal ─────────────────────────────────────────────────────────────

function CreateProjectModal({ onClose, onCreated }: { onClose: () => void; onCreated: (p: Project) => void }) {
  const [name, setName] = useState('')
  const [slug, setSlug] = useState('')
  const [description, setDescription] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)

  const autoSlug = (n: string) =>
    n.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    setCreating(true)
    try {
      const p = await jsonRpcClient.createProject({
        name: name.trim(),
        slug: slug.trim(),
        description: description.trim() || undefined,
      })
      onCreated(p)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create project')
    } finally {
      setCreating(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="bg-[var(--bg)] border border-[var(--border)] rounded-xl shadow-lg w-full max-w-sm p-6 space-y-4" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <h2 className="text-base font-medium text-[var(--text-primary)]">New Project</h2>
          <button onClick={onClose} className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"><X className="w-4 h-4" /></button>
        </div>
        <form onSubmit={handleSubmit} className="space-y-3">
          <div>
            <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-1">Name</label>
            <input type="text" value={name} onChange={e => { setName(e.target.value); setSlug(autoSlug(e.target.value)) }}
              placeholder="CIFAR-10 ResNet Search" className={inputCls} required />
          </div>
          <div>
            <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-1">Slug</label>
            <input type="text" value={slug} onChange={e => setSlug(e.target.value)}
              placeholder="cifar10-resnet-search" className={inputCls} required
              pattern="[a-z0-9-]+" title="Lowercase letters, digits, and hyphens only" />
          </div>
          <div>
            <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-1">Description <span className="normal-case font-normal">(optional)</span></label>
            <input type="text" value={description} onChange={e => setDescription(e.target.value)}
              placeholder="What are agents researching?" className={inputCls} />
          </div>
          {error && <p className="text-xs text-[var(--danger)]">{error}</p>}
          <button type="submit" disabled={creating || !name.trim() || !slug.trim()}
            className="w-full py-2 bg-[var(--accent)] text-white rounded-lg text-sm font-medium disabled:opacity-50 hover:opacity-90 transition-opacity">
            {creating ? 'Creating…' : 'Create Project'}
          </button>
        </form>
      </div>
    </div>
  )
}

// ── Edit modal ────────────────────────────────────────────────────────────────

function EditProjectModal({ project, onClose, onSaved }: { project: Project; onClose: () => void; onSaved: (p: Project) => void }) {
  const [name, setName] = useState(project.name)
  const [slug, setSlug] = useState(project.slug)
  const [description, setDescription] = useState(project.description ?? '')
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    setSaving(true)
    try {
      const p = await jsonRpcClient.updateProject({
        project_id: project.project_id,
        name: name.trim(),
        slug: slug.trim(),
        description: description.trim() || undefined,
      })
      onSaved(p)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save project')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="bg-[var(--bg)] border border-[var(--border)] rounded-xl shadow-lg w-full max-w-sm p-6 space-y-4" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <h2 className="text-base font-medium text-[var(--text-primary)]">Edit Project</h2>
          <button onClick={onClose} className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"><X className="w-4 h-4" /></button>
        </div>
        <form onSubmit={handleSubmit} className="space-y-3">
          <div>
            <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-1">Name</label>
            <input type="text" value={name} onChange={e => setName(e.target.value)}
              className={inputCls} required />
          </div>
          <div>
            <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-1">Slug</label>
            <input type="text" value={slug} onChange={e => setSlug(e.target.value)}
              className={inputCls} required pattern="[a-z0-9-]+" title="Lowercase letters, digits, and hyphens only" />
          </div>
          <div>
            <label className="block text-xs font-medium text-[var(--text-muted)] uppercase tracking-wide mb-1">Description</label>
            <input type="text" value={description} onChange={e => setDescription(e.target.value)}
              placeholder="Optional" className={inputCls} />
          </div>
          {error && <p className="text-xs text-[var(--danger)]">{error}</p>}
          <div className="flex gap-2">
            <button type="button" onClick={onClose}
              className="flex-1 py-2 border border-[var(--border)] text-[var(--text-muted)] rounded-lg text-sm hover:bg-[var(--bg-subtle)] transition-colors">
              Cancel
            </button>
            <button type="submit" disabled={saving || !name.trim() || !slug.trim()}
              className="flex-1 py-2 bg-[var(--accent)] text-white rounded-lg text-sm font-medium disabled:opacity-50 hover:opacity-90 transition-opacity">
              {saving ? 'Saving…' : 'Save'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Delete confirmation ───────────────────────────────────────────────────────

function DeleteConfirmModal({ project, onClose, onDeleted }: { project: Project; onClose: () => void; onDeleted: () => void }) {
  const [deleting, setDeleting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleDelete = async () => {
    setDeleting(true)
    setError(null)
    try {
      await jsonRpcClient.deleteProject(project.project_id)
      onDeleted()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete project')
      setDeleting(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="bg-[var(--bg)] border border-[var(--border)] rounded-xl shadow-lg w-full max-w-sm p-6 space-y-4" onClick={e => e.stopPropagation()}>
        <h2 className="text-base font-medium text-[var(--text-primary)]">Delete Project</h2>
        <p className="text-sm text-[var(--text-muted)]">
          Delete <span className="font-medium text-[var(--text-primary)]">{project.name}</span>?
          Atoms will be preserved but unlinked from this project.
        </p>
        {error && <p className="text-xs text-[var(--danger)]">{error}</p>}
        <div className="flex gap-2">
          <button onClick={onClose}
            className="flex-1 py-2 border border-[var(--border)] text-[var(--text-muted)] rounded-lg text-sm hover:bg-[var(--bg-subtle)] transition-colors">
            Cancel
          </button>
          <button onClick={handleDelete} disabled={deleting}
            className="flex-1 py-2 bg-[var(--danger)] text-white rounded-lg text-sm font-medium disabled:opacity-50 hover:opacity-90 transition-opacity">
            {deleting ? 'Deleting…' : 'Delete'}
          </button>
        </div>
      </div>
    </div>
  )
}

// ── Main switcher ─────────────────────────────────────────────────────────────

type Modal = { type: 'create' } | { type: 'edit'; project: Project } | { type: 'delete'; project: Project }

export default function ProjectSwitcher() {
  const [open, setOpen] = useState(false)
  const [modal, setModal] = useState<Modal | null>(null)
  const { activeProject, setActiveProject } = useActiveProject()
  const queryClient = useQueryClient()

  const { data } = useQuery({
    queryKey: ['projects'],
    queryFn: () => jsonRpcClient.listProjects(),
    staleTime: 30_000,
  })

  const projects = data?.projects ?? []

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ['projects'] })

  const handleSelect = (project: Project | null) => {
    setActiveProject(project)
    setOpen(false)
  }

  const handleCreated = (p: Project) => {
    invalidate()
    setActiveProject(p)
    setModal(null)
  }

  const handleSaved = (p: Project) => {
    invalidate()
    // If the edited project is the active one, refresh its stored data
    if (activeProject?.project_id === p.project_id) setActiveProject(p)
    setModal(null)
  }

  const handleDeleted = (deleted: Project) => {
    invalidate()
    if (activeProject?.project_id === deleted.project_id) setActiveProject(null)
    setModal(null)
  }

  return (
    <>
      <div className="relative">
        <button
          onClick={() => setOpen(o => !o)}
          className="flex items-center gap-2 px-3 py-1.5 rounded border border-[var(--border)] bg-[var(--bg-subtle)] text-sm text-[var(--text-primary)] hover:border-[var(--accent)] transition-colors max-w-[200px]"
        >
          <FolderOpen className="w-3.5 h-3.5 text-[var(--accent)] flex-shrink-0" />
          <span className="truncate">{activeProject ? activeProject.name : 'All Projects'}</span>
          <ChevronDown className="w-3 h-3 text-[var(--text-muted)] flex-shrink-0 ml-auto" />
        </button>

        {open && (
          <>
            {/* backdrop */}
            <div className="fixed inset-0 z-30" onClick={() => setOpen(false)} />
            <div className="absolute right-0 mt-1 w-64 bg-[var(--bg)] border border-[var(--border)] rounded-lg shadow-lg z-40 py-1">
              {/* All projects */}
              <button
                onClick={() => handleSelect(null)}
                className={`w-full text-left px-3 py-2 text-sm transition-colors flex items-center gap-2 ${
                  !activeProject ? 'text-[var(--accent)] font-medium' : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-subtle)]'
                }`}
              >
                {!activeProject && <Check className="w-3.5 h-3.5 flex-shrink-0" />}
                <span className={!activeProject ? '' : 'pl-5'}>All Projects</span>
              </button>

              {projects.length > 0 && <div className="border-t border-[var(--border)] my-1" />}

              {projects.map(p => (
                <div key={p.project_id} className="flex items-center group px-1">
                  <button
                    onClick={() => handleSelect(p)}
                    className={`flex-1 text-left px-2 py-2 text-sm transition-colors truncate rounded flex items-center gap-2 ${
                      activeProject?.project_id === p.project_id
                        ? 'text-[var(--accent)] font-medium'
                        : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-subtle)]'
                    }`}
                  >
                    {activeProject?.project_id === p.project_id && <Check className="w-3.5 h-3.5 flex-shrink-0" />}
                    <span className={activeProject?.project_id === p.project_id ? '' : 'pl-5'}>{p.name}</span>
                  </button>
                  {/* Edit / delete — visible on hover */}
                  <div className="flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity pr-1">
                    <button
                      onClick={e => { e.stopPropagation(); setOpen(false); setModal({ type: 'edit', project: p }) }}
                      className="p-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors rounded"
                      title="Edit project"
                    >
                      <Pencil className="w-3 h-3" />
                    </button>
                    <button
                      onClick={e => { e.stopPropagation(); setOpen(false); setModal({ type: 'delete', project: p }) }}
                      className="p-1 text-[var(--text-muted)] hover:text-[var(--danger)] transition-colors rounded"
                      title="Delete project"
                    >
                      <Trash2 className="w-3 h-3" />
                    </button>
                  </div>
                </div>
              ))}

              <div className="border-t border-[var(--border)] my-1" />
              <button
                onClick={() => { setOpen(false); setModal({ type: 'create' }) }}
                className="w-full text-left px-3 py-2 text-sm text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-subtle)] transition-colors flex items-center gap-2"
              >
                <Plus className="w-3.5 h-3.5" />
                New Project
              </button>
            </div>
          </>
        )}
      </div>

      {modal?.type === 'create' && (
        <CreateProjectModal onClose={() => setModal(null)} onCreated={handleCreated} />
      )}
      {modal?.type === 'edit' && (
        <EditProjectModal project={modal.project} onClose={() => setModal(null)} onSaved={handleSaved} />
      )}
      {modal?.type === 'delete' && (
        <DeleteConfirmModal project={modal.project} onClose={() => setModal(null)} onDeleted={() => handleDeleted(modal.project)} />
      )}
    </>
  )
}
