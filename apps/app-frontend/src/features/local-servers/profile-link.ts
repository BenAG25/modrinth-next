import { get_project } from '@/helpers/cache.js'
import {
	install_create_instance,
	installJobInstanceId,
	wait_for_install_job,
} from '@/helpers/install'
import {
	get,
	get_installed_project_ids,
	get_projects,
	install_project_with_dependencies,
} from '@/helpers/instance'
import type { GameInstance, InstanceLoader } from '@/helpers/types'
import { ensureManagedServerWorldExists, start_join_server } from '@/helpers/worlds'
import {
	installLocalServerProfileProjects,
	linkLocalServerInstance,
} from '@/platform/app-lib/local-servers/commands'
import type {
	LocalServer,
	LocalServerInstallReport,
	LocalServerLoader,
} from '@/platform/app-lib/local-servers/types'
import type { AppEvents } from '@/providers/app-events'

/** Loader a game profile needs to join a server with the given software. */
export function profileLoaderFor(loader: LocalServerLoader): InstanceLoader {
	if (loader === 'paper') return 'vanilla'
	return loader
}

/** Server software that matches a game profile. `null` if not supported. */
export function serverLoaderFor(loader: InstanceLoader): LocalServerLoader | null {
	if (loader === 'quilt') return null
	return loader
}

/** Only mod servers share their content with the game profile. */
export function sharesMods(loader: LocalServerLoader) {
	return loader === 'fabric' || loader === 'forge' || loader === 'neoforge'
}

export function serverAddress(server: LocalServer) {
	return server.port === 25565 ? 'localhost' : `localhost:${server.port}`
}

export interface ProfileSyncReport {
	/** Mods newly installed in the game profile */
	toProfile: string[]
	/** Server-only mods that were not copied to the profile */
	serverOnly: string[]
	/** Mods newly installed on the server */
	toServer: LocalServerInstallReport | null
	failed: string[]
}

/** Installs the server's mods in the profile (only the ones the client needs). */
async function serverModsToProfile(
	server: LocalServer,
	instanceId: string,
	report: ProfileSyncReport,
) {
	if (!sharesMods(server.loader)) return
	const installed = new Set(await get_installed_project_ids(instanceId))
	for (const project of server.projects) {
		if (installed.has(project.project_id)) continue
		try {
			const meta = (await get_project(project.project_id)) as {
				client_side?: string
			} | null
			if (meta?.client_side === 'unsupported') {
				report.serverOnly.push(project.title)
				continue
			}
			await install_project_with_dependencies(instanceId, {
				project_id: project.project_id,
				version_id: project.version_id,
				content_type: 'mod',
			})
			report.toProfile.push(project.title)
		} catch (e) {
			console.warn('[LocalServers] Failed to install in profile', project.title, e)
			report.failed.push(project.title)
		}
	}
}

/** Installs the profile's mods on the server (client-only mods are skipped). */
async function profileModsToServer(
	server: LocalServer,
	instanceId: string,
	report: ProfileSyncReport,
) {
	if (!sharesMods(server.loader)) return
	const files = await get_projects(instanceId)
	const projects = Object.entries(files)
		.filter(([path, file]) => {
			const normalized = path.replace(/\\/g, '/')
			return normalized.startsWith('mods/') && file.enabled && !!file.metadata?.project_id
		})
		.map(([, file]) => ({
			project_id: file.metadata!.project_id,
			version_id: file.metadata!.version_id ?? null,
		}))
	if (projects.length === 0) return
	report.toServer = await installLocalServerProfileProjects(server.id, projects)
}

/** Syncs mods in both directions so profile and server match. */
export async function syncProfileAndServer(
	server: LocalServer,
	instanceId: string,
): Promise<ProfileSyncReport> {
	const report: ProfileSyncReport = { toProfile: [], serverOnly: [], toServer: null, failed: [] }
	await profileModsToServer(server, instanceId, report)
	await serverModsToProfile(server, instanceId, report)
	return report
}

/** Mirrors the server's mods into the profile only. */
export async function syncServerToProfile(
	server: LocalServer,
	instanceId: string,
): Promise<ProfileSyncReport> {
	const report: ProfileSyncReport = { toProfile: [], serverOnly: [], toServer: null, failed: [] }
	await serverModsToProfile(server, instanceId, report)
	return report
}

/** Mirrors the profile's mods onto the server only. */
export async function syncProfileToServer(
	server: LocalServer,
	instanceId: string,
): Promise<ProfileSyncReport> {
	const report: ProfileSyncReport = { toProfile: [], serverOnly: [], toServer: null, failed: [] }
	await profileModsToServer(server, instanceId, report)
	return report
}

/**
 * Creates a game profile that matches the server (same Minecraft version and
 * loader), installs the client-side mods, adds the server to the profile's
 * server list and links both.
 */
export async function createProfileForServer(
	server: LocalServer,
	appEvents: AppEvents,
): Promise<{ instanceId: string; report: ProfileSyncReport; server: LocalServer }> {
	const loader = profileLoaderFor(server.loader)
	const job = await install_create_instance({
		name: server.name,
		gameVersion: server.game_version,
		loader,
		loaderVersion: loader === 'vanilla' ? null : (server.loader_version ?? 'stable'),
		iconPath: null,
	})
	const instanceId = installJobInstanceId(job)
	if (!instanceId) throw new Error('Das Spielprofil konnte nicht erstellt werden')
	await wait_for_install_job(appEvents, job.job_id)

	const linked = await linkLocalServerInstance(server.id, instanceId)
	await ensureManagedServerWorldExists(instanceId, server.name, serverAddress(server))
	const report = await syncServerToProfile(linked, instanceId)
	return { instanceId, report, server: linked }
}

/** Returns the linked profile, or `null` if it no longer exists. */
export async function getLinkedProfile(server: LocalServer): Promise<GameInstance | null> {
	if (!server.linked_instance) return null
	try {
		return await get(server.linked_instance)
	} catch {
		return null
	}
}

export async function playOnServer(server: LocalServer) {
	if (!server.linked_instance) throw new Error('Kein Spielprofil verknüpft')
	await start_join_server(server.linked_instance, serverAddress(server))
}

/** Short German summary for notifications. */
export function describeSync(report: ProfileSyncReport): string {
	const parts: string[] = []
	const toServer = report.toServer?.installed ?? []
	if (toServer.length) parts.push(`Auf den Server: ${toServer.join(', ')}`)
	if (report.toProfile.length) parts.push(`Ins Profil: ${report.toProfile.join(', ')}`)
	const clientOnly = report.toServer?.client_only ?? []
	if (clientOnly.length) parts.push(`Nur im Profil (Client-Mods): ${clientOnly.join(', ')}`)
	if (report.serverOnly.length) parts.push(`Nur auf dem Server: ${report.serverOnly.join(', ')}`)
	const failed = [...report.failed, ...(report.toServer?.failed ?? [])]
	if (failed.length) parts.push(`Nicht möglich: ${failed.join(', ')}`)
	return parts.length ? parts.join('\n') : 'Profil und Server haben bereits dieselben Mods.'
}
