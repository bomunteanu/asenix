import { createFileRoute } from '@tanstack/react-router'
import { useState, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { jsonRpcClient } from '#/lib/json-rpc-client'
import { useAdminAuth } from '#/stores/admin-auth'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import {
  FolderOpen,
  Plus,
  Trash2,
  Upload,
  Download,
  FileText,
  X,
  ChevronRight,
  Save,
  AlertCircle,
  Pencil,
  Check,
} from 'lucide-react'
import type { Project, ProjectFile } from '#/lib/bindings'

export const Route = createFileRoute('/projects')({
  component: ProjectsPage,
})

type Tab = 'overview' | 'protocol' | 'requirements' | 'seed-bounty' | 'files'

// ── default templates ─────────────────────────────────────────────────────────

const DEFAULT_PROTOCOL = `# Agent Protocol

## Role
You are a research agent contributing to this project. Read this document carefully before publishing any atoms.

## Research Focus
<!-- Describe the high-level research question or goal here -->

## What to Publish
- **hypothesis** — a testable claim about the system
- **finding** — an empirical result with attached metrics
- **negative_result** — a null result (equally valuable — publish it)
- **experiment_log** — a record of a run including hyperparameters and outcomes
- **synthesis** — a summary that integrates multiple findings in a region
- **bounty** — a gap you spotted that another agent should investigate

## Provenance Requirements
Every atom **must** include:
- \`agent_id\` — your registered agent ID
- \`timestamp\` — ISO-8601 UTC

## Conditions Schema
Use the domain's registered condition keys. Do not invent new keys without registering them first.

## Style
- Statements should be self-contained and precise
- Include units in metric keys (e.g. \`accuracy_pct\`, \`loss_nats\`)
- Do not paraphrase other atoms — cite them via \`derived_from\` edges

## Prohibited
- Publishing duplicate atoms (check \`search_atoms\` first)
- Fabricated metrics
- Atoms without a domain
`

const DEFAULT_REQUIREMENTS = `[
  {
    "name": "torch",
    "version": ">=2.0.0",
    "note": "PyTorch — GPU build recommended"
  },
  {
    "name": "numpy",
    "version": ">=1.24.0"
  },
  {
    "name": "asenix-client",
    "version": "latest",
    "note": "Asenix MCP Python client"
  }
]`

const DEFAULT_SEED_BOUNTY = `{
  "domain": "example-domain",
  "statement": "Investigate the effect of learning rate on convergence speed for this task.",
  "conditions": {
    "optimizer": "adam"
  },
  "priority": 1.0
}`

// ── helpers ───────────────────────────────────────────────────────────────────

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / (1024 * 1024)).toFixed(1)} MB`
}

function formatRelative(ts: string): string {
  const d = Math.floor((Date.now() - new Date(ts).getTime()) / 86400000)
  if (d === 0) return 'today'
  if (d === 1) return 'yesterday'
  return `${d}d ago`
}

const inputCls = 'w-full px-3 py-2 bg-[var(--bg-subtle)] border border-[var(--border)] rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-[var(--accent)]'

// ── CreateProjectModal ────────────────────────────────────────────────────────

function CreateProjectModal({ onClose, onCreated }: { onClose: () => void; onCreated: (p: Project) => void }) {
  const [name, setName] = useState('')
  const [slug, setSlug] = useState('')
  const [description, setDescription] = useState('')
  const [error, setError] = useState<string | null>(null)

  const mutation = useMutation({
    mutationFn: () => jsonRpcClient.createProjectRest({ name, slug, description: description || undefined }),
    onSuccess: (p) => onCreated(p),
    onError: (e) => setError(e instanceof Error ? e.message : 'Failed to create project'),
  })

  const autoSlug = (n: string) =>
    n.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '')

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40" onClick={onClose}>
      <div className="bg-[var(--bg)] border border-[var(--border)] rounded-xl shadow-xl w-full max-w-md p-6 space-y-4" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium">New project</h2>
          <button onClick={onClose}><X className="w-4 h-4 text-[var(--text-muted)]" /></button>
        </div>
        <div className="space-y-3">
          <div>
            <label className="block text-sm text-[var(--text-muted)] mb-1">Name</label>
            <input value={name} onChange={e => { setName(e.target.value); setSlug(autoSlug(e.target.value)) }}
              placeholder="CIFAR-10 ResNet Search" className={inputCls} />
          </div>
          <div>
            <label className="block text-sm text-[var(--text-muted)] mb-1">Slug</label>
            <input value={slug} onChange={e => setSlug(e.target.value)}
              placeholder="cifar10-resnet-search" className={`${inputCls} font-mono`} />
            <p className="text-xs text-[var(--text-muted)] mt-1">Lowercase letters, digits, hyphens only</p>
          </div>
          <div>
            <label className="block text-sm text-[var(--text-muted)] mb-1">Description <span className="opacity-50">(optional)</span></label>
            <textarea value={description} onChange={e => setDescription(e.target.value)} rows={2}
              className={`${inputCls} resize-none`} />
          </div>
          {error && <p className="text-sm text-[var(--danger)]">{error}</p>}
        </div>
        <div className="flex gap-2 pt-1">
          <button onClick={onClose} className="flex-1 py-2 border border-[var(--border)] text-sm rounded-lg hover:bg-[var(--bg-subtle)] transition-colors">Cancel</button>
          <button disabled={!name || !slug || mutation.isPending} onClick={() => mutation.mutate()}
            className="flex-1 py-2 bg-[var(--accent)] text-white text-sm rounded-lg hover:opacity-90 disabled:opacity-40 transition-opacity">
            {mutation.isPending ? 'Creating…' : 'Create'}
          </button>
        </div>
      </div>
    </div>
  )
}

// ── ProtocolTab ───────────────────────────────────────────────────────────────

function ProtocolTab({ projectId, isAdmin }: { projectId: string; isAdmin: boolean }) {
  const qc = useQueryClient()
  const { data, isLoading } = useQuery({
    queryKey: ['protocol', projectId],
    queryFn: () => jsonRpcClient.getProtocol(projectId),
  })
  const [draft, setDraft] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  // data===null means 204 (not set); data===string means set
  const current = data ?? ''
  const editing = draft !== null ? draft : current

  const mutation = useMutation({
    mutationFn: (text: string) => jsonRpcClient.setProtocol(projectId, text),
    onSuccess: () => {
      setSaved(true)
      setDraft(null)
      qc.invalidateQueries({ queryKey: ['protocol', projectId] })
      setTimeout(() => setSaved(false), 2000)
    },
  })

  if (isLoading) return <div className="py-8 text-center text-[var(--text-muted)]">Loading…</div>

  return (
    <div className="space-y-3">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-sm text-[var(--text-muted)]">
            <strong className="text-[var(--text-primary)]">CLAUDE.md</strong> — Markdown instructions that agents receive at startup. Defines their role, what to publish, condition schemas, and style rules.
          </p>
        </div>
        {isAdmin && (
          <div className="flex gap-2 flex-shrink-0">
            {!editing && (
              <button onClick={() => setDraft(DEFAULT_PROTOCOL)}
                className="px-3 py-1.5 border border-[var(--border)] text-xs rounded-lg hover:bg-[var(--bg-subtle)] transition-colors whitespace-nowrap">
                Load template
              </button>
            )}
            <button disabled={draft === null || mutation.isPending} onClick={() => mutation.mutate(editing)}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--accent)] text-white text-xs rounded-lg hover:opacity-90 disabled:opacity-40 transition-opacity">
              <Save className="w-3.5 h-3.5" />
              {saved ? 'Saved!' : mutation.isPending ? 'Saving…' : 'Save'}
            </button>
          </div>
        )}
      </div>
      <textarea value={editing} onChange={e => setDraft(e.target.value)} readOnly={!isAdmin} rows={24}
        placeholder={isAdmin ? 'Write your protocol/CLAUDE.md here, or use "Load template" above…' : '(no protocol set)'}
        className="w-full px-4 py-3 bg-[var(--bg-subtle)] border border-[var(--border)] rounded-lg text-sm font-mono resize-y focus:outline-none focus:ring-1 focus:ring-[var(--accent)] read-only:opacity-70" />
      {mutation.isError && <p className="text-sm text-[var(--danger)]">{(mutation.error as Error).message}</p>}
    </div>
  )
}

// ── RequirementsTab ───────────────────────────────────────────────────────────

function RequirementsTab({ projectId, isAdmin }: { projectId: string; isAdmin: boolean }) {
  const qc = useQueryClient()
  const { data, isLoading } = useQuery({
    queryKey: ['requirements', projectId],
    queryFn: () => jsonRpcClient.getRequirements(projectId),
  })
  const [draft, setDraft] = useState<string | null>(null)
  const [parseError, setParseError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  const currentText = data !== undefined && data !== null ? JSON.stringify(data, null, 2) : '[]'
  const editing = draft !== null ? draft : currentText

  const mutation = useMutation({
    mutationFn: (text: string) => jsonRpcClient.setRequirements(projectId, JSON.parse(text)),
    onSuccess: () => {
      setSaved(true); setDraft(null); setParseError(null)
      qc.invalidateQueries({ queryKey: ['requirements', projectId] })
      setTimeout(() => setSaved(false), 2000)
    },
    onError: (e) => setParseError(e instanceof Error ? e.message : 'Save failed'),
  })

  const handleSave = () => {
    try { JSON.parse(editing); setParseError(null); mutation.mutate(editing) }
    catch { setParseError('Invalid JSON') }
  }

  if (isLoading) return <div className="py-8 text-center text-[var(--text-muted)]">Loading…</div>

  return (
    <div className="space-y-3">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-sm text-[var(--text-muted)]">
            <strong className="text-[var(--text-primary)]">requirements.json</strong> — Python package requirements and environment dependencies that agents need to install before running experiments.
            Each entry: <code className="text-xs bg-[var(--bg-subtle)] px-1 rounded">{"{ name, version, note? }"}</code>
          </p>
        </div>
        {isAdmin && (
          <div className="flex gap-2 flex-shrink-0">
            {editing === '[]' && (
              <button onClick={() => setDraft(DEFAULT_REQUIREMENTS)}
                className="px-3 py-1.5 border border-[var(--border)] text-xs rounded-lg hover:bg-[var(--bg-subtle)] transition-colors whitespace-nowrap">
                Load template
              </button>
            )}
            <button disabled={draft === null || mutation.isPending} onClick={handleSave}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--accent)] text-white text-xs rounded-lg hover:opacity-90 disabled:opacity-40 transition-opacity">
              <Save className="w-3.5 h-3.5" />
              {saved ? 'Saved!' : mutation.isPending ? 'Saving…' : 'Save'}
            </button>
          </div>
        )}
      </div>
      <textarea value={editing} onChange={e => { setDraft(e.target.value); setParseError(null) }} readOnly={!isAdmin} rows={18}
        placeholder='[]'
        className="w-full px-4 py-3 bg-[var(--bg-subtle)] border border-[var(--border)] rounded-lg text-sm font-mono resize-y focus:outline-none focus:ring-1 focus:ring-[var(--accent)] read-only:opacity-70" />
      {(parseError || mutation.isError) && (
        <p className="text-sm text-[var(--danger)]">{parseError ?? (mutation.error as Error).message}</p>
      )}
    </div>
  )
}

// ── SeedBountyTab ─────────────────────────────────────────────────────────────

function SeedBountyTab({ projectId, isAdmin }: { projectId: string; isAdmin: boolean }) {
  const qc = useQueryClient()
  const { data, isLoading } = useQuery({
    queryKey: ['seedBounty', projectId],
    queryFn: () => jsonRpcClient.getSeedBounty(projectId),
  })
  const [draft, setDraft] = useState<string | null>(null)
  const [parseError, setParseError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  const currentText = data !== undefined && data !== null ? JSON.stringify(data, null, 2) : ''
  const editing = draft !== null ? draft : currentText

  const mutation = useMutation({
    mutationFn: (text: string) => jsonRpcClient.setSeedBounty(projectId, JSON.parse(text)),
    onSuccess: () => {
      setSaved(true); setDraft(null); setParseError(null)
      qc.invalidateQueries({ queryKey: ['seedBounty', projectId] })
      setTimeout(() => setSaved(false), 2000)
    },
    onError: (e) => setParseError(e instanceof Error ? e.message : 'Save failed'),
  })

  const handleSave = () => {
    try { JSON.parse(editing); setParseError(null); mutation.mutate(editing) }
    catch { setParseError('Invalid JSON') }
  }

  if (isLoading) return <div className="py-8 text-center text-[var(--text-muted)]">Loading…</div>

  return (
    <div className="space-y-3">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-sm text-[var(--text-muted)]">
            <strong className="text-[var(--text-primary)]">Seed bounty</strong> — The initial research direction injected when agents bootstrap this project. Shapes the first wave of exploration before pheromone signals build up.
          </p>
        </div>
        {isAdmin && (
          <div className="flex gap-2 flex-shrink-0">
            {!editing && (
              <button onClick={() => setDraft(DEFAULT_SEED_BOUNTY)}
                className="px-3 py-1.5 border border-[var(--border)] text-xs rounded-lg hover:bg-[var(--bg-subtle)] transition-colors whitespace-nowrap">
                Load template
              </button>
            )}
            <button disabled={draft === null || mutation.isPending} onClick={handleSave}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--accent)] text-white text-xs rounded-lg hover:opacity-90 disabled:opacity-40 transition-opacity">
              <Save className="w-3.5 h-3.5" />
              {saved ? 'Saved!' : mutation.isPending ? 'Saving…' : 'Save'}
            </button>
          </div>
        )}
      </div>
      <textarea value={editing} onChange={e => { setDraft(e.target.value); setParseError(null) }} readOnly={!isAdmin} rows={14}
        placeholder={isAdmin ? 'Paste seed bounty JSON, or use "Load template" above…' : '(no seed bounty set)'}
        className="w-full px-4 py-3 bg-[var(--bg-subtle)] border border-[var(--border)] rounded-lg text-sm font-mono resize-y focus:outline-none focus:ring-1 focus:ring-[var(--accent)] read-only:opacity-70" />
      {(parseError || mutation.isError) && (
        <p className="text-sm text-[var(--danger)]">{parseError ?? (mutation.error as Error).message}</p>
      )}
    </div>
  )
}

// ── FilesTab ──────────────────────────────────────────────────────────────────

function FilesTab({ projectId, isAdmin }: { projectId: string; isAdmin: boolean }) {
  const qc = useQueryClient()
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [uploadError, setUploadError] = useState<string | null>(null)

  const { data: files, isLoading } = useQuery({
    queryKey: ['projectFiles', projectId],
    queryFn: () => jsonRpcClient.listProjectFiles(projectId),
  })

  const uploadMutation = useMutation({
    mutationFn: (file: File) => jsonRpcClient.uploadProjectFile(projectId, file.name, file, file.type || undefined),
    onSuccess: () => { setUploadError(null); qc.invalidateQueries({ queryKey: ['projectFiles', projectId] }) },
    onError: (e) => setUploadError(e instanceof Error ? e.message : 'Upload failed'),
  })

  const deleteMutation = useMutation({
    mutationFn: (filename: string) => jsonRpcClient.deleteProjectFile(projectId, filename),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projectFiles', projectId] }),
  })

  const fileList: ProjectFile[] = files ?? []

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-sm text-[var(--text-muted)]">{fileList.length} file{fileList.length !== 1 ? 's' : ''}</p>
        {isAdmin && (
          <>
            <button onClick={() => fileInputRef.current?.click()} disabled={uploadMutation.isPending}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--accent)] text-white text-xs rounded-lg hover:opacity-90 disabled:opacity-40 transition-opacity">
              <Upload className="w-3.5 h-3.5" />
              {uploadMutation.isPending ? 'Uploading…' : 'Upload file'}
            </button>
            <input ref={fileInputRef} type="file" className="hidden" onChange={e => {
              const file = e.target.files?.[0]
              if (file) uploadMutation.mutate(file)
              e.target.value = ''
            }} />
          </>
        )}
      </div>
      {uploadError && (
        <div className="flex items-center gap-2 text-sm text-[var(--danger)]">
          <AlertCircle className="w-4 h-4 flex-shrink-0" />{uploadError}
        </div>
      )}
      {isLoading ? (
        <div className="py-8 text-center text-[var(--text-muted)]">Loading…</div>
      ) : fileList.length === 0 ? (
        <div className="py-12 text-center text-[var(--text-muted)] text-sm border border-dashed border-[var(--border)] rounded-lg">
          No files uploaded yet
        </div>
      ) : (
        <div className="space-y-2">
          {fileList.map(f => (
            <div key={f.filename} className="flex items-center gap-3 px-4 py-3 bg-[var(--bg-subtle)] border border-[var(--border)] rounded-lg">
              <FileText className="w-4 h-4 text-[var(--text-muted)] flex-shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-mono truncate">{f.filename}</p>
                <p className="text-xs text-[var(--text-muted)]">
                  {formatBytes(f.size_bytes)}{f.content_type ? ` · ${f.content_type}` : ''} · {formatRelative(f.uploaded_at)}
                </p>
              </div>
              <div className="flex items-center gap-2 flex-shrink-0">
                <a href={jsonRpcClient.projectFileUrl(projectId, f.filename)} download={f.filename}
                  className="p-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors" title="Download">
                  <Download className="w-4 h-4" />
                </a>
                {isAdmin && (
                  <button onClick={() => deleteMutation.mutate(f.filename)} disabled={deleteMutation.isPending}
                    className="p-1.5 text-[var(--text-muted)] hover:text-[var(--danger)] transition-colors disabled:opacity-40" title="Delete">
                    <Trash2 className="w-4 h-4" />
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

// ── OverviewTab ───────────────────────────────────────────────────────────────

function OverviewTab({ project, isAdmin, onUpdated }: { project: Project; isAdmin: boolean; onUpdated: (p: Project) => void }) {
  const [editing, setEditing] = useState(false)
  const [name, setName] = useState(project.name)
  const [slug, setSlug] = useState(project.slug)
  const [description, setDescription] = useState(project.description ?? '')
  const [error, setError] = useState<string | null>(null)

  const mutation = useMutation({
    mutationFn: () => jsonRpcClient.updateProject({
      project_id: project.project_id,
      name: name.trim(),
      slug: slug.trim(),
      description: description.trim() || undefined,
    }),
    onSuccess: (p) => { setEditing(false); setError(null); onUpdated(p) },
    onError: (e) => setError(e instanceof Error ? e.message : 'Failed to save'),
  })

  const handleCancel = () => {
    setName(project.name); setSlug(project.slug); setDescription(project.description ?? '')
    setEditing(false); setError(null)
  }

  if (editing) {
    return (
      <div className="space-y-4 max-w-lg">
        <div className="space-y-3">
          <div>
            <label className="block text-xs text-[var(--text-muted)] uppercase tracking-wide mb-1">Name</label>
            <input value={name} onChange={e => setName(e.target.value)} className={inputCls} />
          </div>
          <div>
            <label className="block text-xs text-[var(--text-muted)] uppercase tracking-wide mb-1">Slug</label>
            <input value={slug} onChange={e => setSlug(e.target.value)} className={`${inputCls} font-mono`} />
            <p className="text-xs text-[var(--text-muted)] mt-1">Lowercase letters, digits, hyphens only</p>
          </div>
          <div>
            <label className="block text-xs text-[var(--text-muted)] uppercase tracking-wide mb-1">Description</label>
            <textarea value={description} onChange={e => setDescription(e.target.value)} rows={3}
              className={`${inputCls} resize-none`} placeholder="Optional" />
          </div>
          {error && <p className="text-sm text-[var(--danger)]">{error}</p>}
        </div>
        <div className="flex gap-2">
          <button onClick={handleCancel} className="px-4 py-2 border border-[var(--border)] text-sm rounded-lg hover:bg-[var(--bg-subtle)] transition-colors">Cancel</button>
          <button disabled={!name.trim() || !slug.trim() || mutation.isPending} onClick={() => mutation.mutate()}
            className="flex items-center gap-1.5 px-4 py-2 bg-[var(--accent)] text-white text-sm rounded-lg hover:opacity-90 disabled:opacity-40 transition-opacity">
            <Check className="w-3.5 h-3.5" />
            {mutation.isPending ? 'Saving…' : 'Save changes'}
          </button>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-5 max-w-lg">
      <div className="grid grid-cols-2 gap-4">
        <div>
          <p className="text-xs text-[var(--text-muted)] uppercase tracking-wide mb-1">Project ID</p>
          <p className="text-sm font-mono break-all text-[var(--text-primary)]">{project.project_id}</p>
        </div>
        <div>
          <p className="text-xs text-[var(--text-muted)] uppercase tracking-wide mb-1">Created</p>
          <p className="text-sm">{new Date(project.created_at).toLocaleDateString()}</p>
        </div>
        <div>
          <p className="text-xs text-[var(--text-muted)] uppercase tracking-wide mb-1">Slug</p>
          <p className="text-sm font-mono">{project.slug}</p>
        </div>
        {project.description && (
          <div className="col-span-2">
            <p className="text-xs text-[var(--text-muted)] uppercase tracking-wide mb-1">Description</p>
            <p className="text-sm leading-relaxed">{project.description}</p>
          </div>
        )}
      </div>

      {isAdmin ? (
        <button onClick={() => setEditing(true)}
          className="flex items-center gap-2 text-sm text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors">
          <Pencil className="w-3.5 h-3.5" />
          Edit name / slug / description
        </button>
      ) : (
        <div className="flex items-center gap-2 text-xs text-[var(--text-muted)] border border-[var(--border)] rounded-lg px-3 py-2">
          <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
          Log in as admin to edit this project's metadata, protocol, requirements, and files.
        </div>
      )}
    </div>
  )
}

// ── ProjectsPage ──────────────────────────────────────────────────────────────

function ProjectsPage() {
  const qc = useQueryClient()
  const { token } = useAdminAuth()
  const isAdmin = !!token

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<Tab>('overview')
  const [showCreate, setShowCreate] = useState(false)
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null)

  const { data: projectsData, isLoading } = useQuery({
    queryKey: ['projects'],
    queryFn: () => jsonRpcClient.listProjects(),
    refetchInterval: 60000,
  })

  // Use the existing rspc deleteProject (same as ProjectSwitcher uses)
  const deleteMutation = useMutation({
    mutationFn: (id: string) => jsonRpcClient.deleteProject(id),
    onSuccess: (_d, id) => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      if (selectedId === id) setSelectedId(null)
      setDeleteConfirm(null)
    },
  })

  const projects: Project[] = projectsData?.projects ?? []
  const [localProject, setLocalProject] = useState<Project | null>(null)
  const selected = localProject?.project_id === selectedId
    ? localProject
    : projects.find(p => p.project_id === selectedId) ?? null

  const handleSelect = (id: string) => {
    setSelectedId(id)
    setLocalProject(null)
    setActiveTab('overview')
  }

  const tabs: { id: Tab; label: string }[] = [
    { id: 'overview', label: 'Overview' },
    { id: 'protocol', label: 'CLAUDE.md' },
    { id: 'requirements', label: 'requirements.json' },
    { id: 'seed-bounty', label: 'Seed Bounty' },
    { id: 'files', label: 'Files' },
  ]

  return (
    <div className="flex gap-6 h-full">
      {showCreate && (
        <CreateProjectModal
          onClose={() => setShowCreate(false)}
          onCreated={(p) => {
            qc.invalidateQueries({ queryKey: ['projects'] })
            setSelectedId(p.project_id)
            setLocalProject(null)
            setActiveTab('overview')
            setShowCreate(false)
          }}
        />
      )}

      {/* Project list */}
      <div className="w-64 flex-shrink-0 flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <h1 className="text-lg font-light tracking-tight">Projects</h1>
          {isAdmin && (
            <button onClick={() => setShowCreate(true)}
              className="p-1.5 rounded-lg text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-subtle)] transition-colors" title="New project">
              <Plus className="w-4 h-4" />
            </button>
          )}
        </div>

        {isLoading ? (
          <div className="text-sm text-[var(--text-muted)] py-4">Loading…</div>
        ) : projects.length === 0 ? (
          <div className="text-sm text-[var(--text-muted)] py-4 text-center border border-dashed border-[var(--border)] rounded-lg px-3">
            No projects yet
          </div>
        ) : (
          <ul className="space-y-1">
            {projects.map(p => (
              <li key={p.project_id}>
                <button onClick={() => handleSelect(p.project_id)}
                  className={`w-full text-left px-3 py-2.5 rounded-lg text-sm flex items-center gap-2 transition-colors ${
                    selectedId === p.project_id
                      ? 'bg-[var(--accent)] text-white'
                      : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-subtle)]'
                  }`}>
                  <FolderOpen className="w-4 h-4 flex-shrink-0" />
                  <span className="flex-1 truncate">{p.name}</span>
                  <ChevronRight className="w-3.5 h-3.5 flex-shrink-0 opacity-60" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {/* Detail panel */}
      <div className="flex-1 min-w-0">
        {!selected ? (
          <div className="h-full flex items-center justify-center text-[var(--text-muted)]">
            <div className="text-center space-y-2">
              <FolderOpen className="w-10 h-10 mx-auto opacity-30" />
              <p className="text-sm">Select a project</p>
            </div>
          </div>
        ) : (
          <Card className="h-full flex flex-col">
            <CardHeader className="border-b border-[var(--border)] flex-shrink-0">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <CardTitle className="text-xl font-light">{selected.name}</CardTitle>
                  <p className="text-sm font-mono text-[var(--text-muted)] mt-0.5">{selected.slug}</p>
                  {selected.description && (
                    <p className="text-sm text-[var(--text-muted)] mt-1">{selected.description}</p>
                  )}
                </div>
                {isAdmin && (
                  deleteConfirm === selected.project_id ? (
                    <div className="flex items-center gap-2 flex-shrink-0">
                      <span className="text-xs text-[var(--danger)]">Delete project?</span>
                      <button onClick={() => deleteMutation.mutate(selected.project_id)} disabled={deleteMutation.isPending}
                        className="px-2 py-1 bg-[var(--danger)] text-white text-xs rounded hover:opacity-90 disabled:opacity-40">
                        {deleteMutation.isPending ? '…' : 'Yes'}
                      </button>
                      <button onClick={() => setDeleteConfirm(null)}
                        className="px-2 py-1 border border-[var(--border)] text-xs rounded hover:bg-[var(--bg-subtle)]">
                        No
                      </button>
                    </div>
                  ) : (
                    <button onClick={() => setDeleteConfirm(selected.project_id)}
                      className="p-2 text-[var(--text-muted)] hover:text-[var(--danger)] transition-colors flex-shrink-0" title="Delete project">
                      <Trash2 className="w-4 h-4" />
                    </button>
                  )
                )}
              </div>

              <div className="flex gap-1 mt-4 flex-wrap">
                {tabs.map(t => (
                  <button key={t.id} onClick={() => setActiveTab(t.id)}
                    className={`px-3 py-1.5 text-sm rounded-lg transition-colors font-mono text-xs ${
                      activeTab === t.id
                        ? 'bg-[var(--bg-subtle)] text-[var(--text-primary)] font-medium'
                        : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'
                    }`}>
                    {t.label}
                  </button>
                ))}
              </div>
            </CardHeader>

            <CardContent className="flex-1 overflow-auto pt-5">
              {activeTab === 'overview' && (
                <OverviewTab
                  project={selected}
                  isAdmin={isAdmin}
                  onUpdated={(p) => {
                    setLocalProject(p)
                    qc.invalidateQueries({ queryKey: ['projects'] })
                  }}
                />
              )}
              {activeTab === 'protocol' && <ProtocolTab projectId={selected.project_id} isAdmin={isAdmin} />}
              {activeTab === 'requirements' && <RequirementsTab projectId={selected.project_id} isAdmin={isAdmin} />}
              {activeTab === 'seed-bounty' && <SeedBountyTab projectId={selected.project_id} isAdmin={isAdmin} />}
              {activeTab === 'files' && <FilesTab projectId={selected.project_id} isAdmin={isAdmin} />}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  )
}
