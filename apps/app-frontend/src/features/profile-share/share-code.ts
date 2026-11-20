/**
 * Modrinth Next: share a game profile as a self-contained code.
 *
 * The code contains the profile's Minecraft version, loader and the Modrinth
 * versions of all enabled mods, resource packs and shaders, compressed and
 * base64url-encoded. No account or server is needed. A newer code for the
 * same profile (same share id) updates a profile imported earlier.
 */
import { get_version_many } from '@/helpers/cache.js'
import {
	install_create_instance,
	install_existing_instance,
	installJobInstanceId,
	wait_for_install_job,
} from '@/helpers/install'
import {
	add_project_from_version,
	edit,
	get,
	get_projects,
	remove_project,
} from '@/helpers/instance'
import type { InstanceLoader } from '@/helpers/types'
import type { AppEvents } from '@/providers/app-events'

const CODE_PREFIX = 'MN1.'
const OWNER_IDS_KEY = 'modrinth-next:share-owner-ids'
const IMPORTS_KEY = 'modrinth-next:share-imports'
const CONTENT_FOLDERS = ['mods/', 'resourcepacks/', 'shaderpacks/']

export interface SharePayload {
	v: 1
	/** Stable id of the shared profile, the same for every update */
	id: string
	/** Unix seconds when the code was created */
	rev: number
	name: string
	game: string
	loader: InstanceLoader
	lv: string | null
	/** Modrinth version ids */
	files: string[]
}

export interface ShareCodeResult {
	code: string
	payload: SharePayload
	/** Files that are not from Modrinth and can't be part of the code */
	skipped: number
}

interface ImportRecord {
	instanceId: string
	rev: number
	projects: string[]
}

// ---------------------------------------------------------------------------
// Local bookkeeping (per device)
// ---------------------------------------------------------------------------

function readJson<T>(key: string, fallback: T): T {
	try {
		const raw = localStorage.getItem(key)
		return raw ? (JSON.parse(raw) as T) : fallback
	} catch {
		return fallback
	}
}

function writeJson(key: string, value: unknown) {
	try {
		localStorage.setItem(key, JSON.stringify(value))
	} catch {
		// Storage unavailable: updates fall back to picking the profile manually.
	}
}

function randomId() {
	const alphabet = '0123456789abcdefghijklmnopqrstuvwxyz'
	const bytes = crypto.getRandomValues(new Uint8Array(10))
	return Array.from(bytes, (b) => alphabet[b % alphabet.length]).join('')
}

function ownerShareId(instanceId: string) {
	const ids = readJson<Record<string, string>>(OWNER_IDS_KEY, {})
	if (!ids[instanceId]) {
		ids[instanceId] = randomId()
		writeJson(OWNER_IDS_KEY, ids)
	}
	return ids[instanceId]
}

export function getImportRecord(shareId: string): ImportRecord | null {
	return readJson<Record<string, ImportRecord>>(IMPORTS_KEY, {})[shareId] ?? null
}

function saveImportRecord(shareId: string, record: ImportRecord) {
	const all = readJson<Record<string, ImportRecord>>(IMPORTS_KEY, {})
	all[shareId] = record
	writeJson(IMPORTS_KEY, all)
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

async function compress(text: string): Promise<Uint8Array> {
	const stream = new Blob([text]).stream().pipeThrough(new CompressionStream('deflate-raw'))
	return new Uint8Array(await new Response(stream).arrayBuffer())
}

async function decompress(bytes: Uint8Array): Promise<string> {
	const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw'))
	return await new Response(stream).text()
}

function toBase64Url(bytes: Uint8Array) {
	let binary = ''
	for (const b of bytes) binary += String.fromCharCode(b)
	return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

function fromBase64Url(text: string) {
	const base64 = text.replace(/-/g, '+').replace(/_/g, '/')
	const padded = base64 + '='.repeat((4 - (base64.length % 4)) % 4)
	return Uint8Array.from(atob(padded), (c) => c.charCodeAt(0))
}

export function isOfflineShareCode(text: string) {
	return text.trim().startsWith(CODE_PREFIX)
}

export async function decodeShareCode(text: string): Promise<SharePayload> {
	const trimmed = text.replace(/\s+/g, '')
	if (!trimmed.startsWith(CODE_PREFIX)) throw new Error('Das ist kein Modrinth-Next-Code.')
	let payload: SharePayload
	try {
		payload = JSON.parse(await decompress(fromBase64Url(trimmed.slice(CODE_PREFIX.length))))
	} catch {
		throw new Error('Der Code ist unvollständig oder beschädigt. Kopiere ihn bitte komplett.')
	}
	if (payload.v !== 1 || !payload.id || !payload.game || !Array.isArray(payload.files)) {
		throw new Error('Dieser Code stammt von einer neueren Version von Modrinth Next.')
	}
	return payload
}

function contentEntries(files: Awaited<ReturnType<typeof get_projects>>) {
	return Object.entries(files)
		.map(([path, file]) => ({ path: path.replace(/\\/g, '/'), file }))
		.filter(({ path }) => CONTENT_FOLDERS.some((folder) => path.startsWith(folder)))
}

/** Creates a share code for a profile. */
export async function createShareCode(instanceId: string): Promise<ShareCodeResult> {
	const instance = await get(instanceId)
	if (!instance) throw new Error('Profil nicht gefunden')
	const entries = contentEntries(await get_projects(instanceId)).filter(({ file }) => file.enabled)
	const files = entries.map(({ file }) => file.metadata?.version_id).filter((v): v is string => !!v)
	const payload: SharePayload = {
		v: 1,
		id: ownerShareId(instanceId),
		rev: Math.floor(Date.now() / 1000),
		name: instance.name,
		game: instance.game_version,
		loader: instance.loader,
		lv: instance.loader_version ?? null,
		files: [...new Set(files)],
	}
	const code = CODE_PREFIX + toBase64Url(await compress(JSON.stringify(payload)))
	return { code, payload, skipped: entries.length - files.length }
}

// ---------------------------------------------------------------------------
// Importing
// ---------------------------------------------------------------------------

export interface ApplyReport {
	added: number
	updated: number
	removed: number
	failed: number
}

type Progress = (message: string) => void

async function applyContent(
	instanceId: string,
	payload: SharePayload,
	previousProjects: string[],
	onProgress: Progress,
): Promise<{ report: ApplyReport; projects: string[] }> {
	const report: ApplyReport = { added: 0, updated: 0, removed: 0, failed: 0 }

	onProgress('Mod-Informationen werden geladen…')
	const versions = ((await get_version_many(payload.files)) ?? []) as {
		id: string
		project_id: string
	}[]
	report.failed += payload.files.length - versions.length

	const installed = new Map<string, { path: string; versionId: string }>()
	for (const { path, file } of contentEntries(await get_projects(instanceId))) {
		if (file.metadata?.project_id) {
			installed.set(file.metadata.project_id, { path, versionId: file.metadata.version_id })
		}
	}

	let done = 0
	for (const version of versions) {
		done++
		const existing = installed.get(version.project_id)
		if (existing?.versionId === version.id) continue
		onProgress(`Inhalte werden installiert (${done}/${versions.length})…`)
		try {
			if (existing) await remove_project(instanceId, existing.path)
			await add_project_from_version(instanceId, version.id, 'standalone')
			if (existing) report.updated++
			else report.added++
		} catch (e) {
			console.warn('[ProfileShare] Failed to install', version.id, e)
			report.failed++
		}
	}

	// Remove content that was part of an earlier code but not of this one.
	// Mods the friend added themselves are never touched.
	const projects = versions.map((v) => v.project_id)
	const keep = new Set(projects)
	for (const projectId of previousProjects) {
		const existing = installed.get(projectId)
		if (keep.has(projectId) || !existing) continue
		try {
			await remove_project(instanceId, existing.path)
			report.removed++
		} catch (e) {
			console.warn('[ProfileShare] Failed to remove', projectId, e)
		}
	}
	return { report, projects }
}

/** Creates a new profile from a share code. Returns the new instance id. */
export async function importAsNewProfile(
	payload: SharePayload,
	appEvents: AppEvents,
	onProgress: Progress,
): Promise<{ instanceId: string; report: ApplyReport }> {
	onProgress('Profil wird erstellt…')
	const job = await install_create_instance({
		name: payload.name,
		gameVersion: payload.game,
		loader: payload.loader,
		loaderVersion: payload.loader === 'vanilla' ? null : (payload.lv ?? 'stable'),
		iconPath: null,
	})
	const instanceId = installJobInstanceId(job)
	if (!instanceId) throw new Error('Das Profil konnte nicht erstellt werden')
	onProgress('Minecraft wird installiert…')
	await wait_for_install_job(appEvents, job.job_id)

	const { report, projects } = await applyContent(instanceId, payload, [], onProgress)
	saveImportRecord(payload.id, { instanceId, rev: payload.rev, projects })
	return { instanceId, report }
}

/** Brings an existing profile up to date with a share code. */
export async function updateProfile(
	instanceId: string,
	payload: SharePayload,
	appEvents: AppEvents,
	onProgress: Progress,
): Promise<ApplyReport> {
	const instance = await get(instanceId)
	if (!instance) throw new Error('Profil nicht gefunden')

	const loaderChanged =
		instance.game_version !== payload.game ||
		instance.loader !== payload.loader ||
		(!!payload.lv && instance.loader_version !== payload.lv)
	if (loaderChanged) {
		onProgress(`Profil wird auf ${payload.loader} ${payload.game} umgestellt…`)
		await edit(instanceId, {
			game_version: payload.game,
			loader: payload.loader,
			loader_version: payload.lv ?? undefined,
		})
		const job = await install_existing_instance(instanceId, false)
		await wait_for_install_job(appEvents, job.job_id)
	}

	const record = getImportRecord(payload.id)
	const previous = record?.instanceId === instanceId ? record.projects : []
	const { report, projects } = await applyContent(instanceId, payload, previous, onProgress)
	saveImportRecord(payload.id, { instanceId, rev: payload.rev, projects })
	return report
}

export function describeApply(report: ApplyReport) {
	const parts = [
		report.added && `${report.added} hinzugefügt`,
		report.updated && `${report.updated} aktualisiert`,
		report.removed && `${report.removed} entfernt`,
		report.failed && `${report.failed} nicht verfügbar`,
	].filter(Boolean)
	return parts.length ? parts.join(', ') : 'Alles war schon aktuell.'
}
