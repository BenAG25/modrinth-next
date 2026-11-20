import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type {
	CreateLocalServer,
	LocalServer,
	LocalServerContentFile,
	LocalServerLoader,
	LocalServerNetworkInfo,
	LocalServerSearchResult,
	LocalServerStatus,
	UpdateLocalServerSettings,
} from './types'

const p = (cmd: string) => `plugin:local-servers|${cmd}`

export const listLocalServers = (): Promise<LocalServer[]> => invoke(p('local_servers_list'))

export const getLocalServer = (id: string): Promise<LocalServer> =>
	invoke(p('local_servers_get'), { id })

export const createLocalServer = (request: CreateLocalServer): Promise<LocalServer> =>
	invoke(p('local_servers_create'), { request })

export const deleteLocalServer = (id: string): Promise<void> =>
	invoke(p('local_servers_delete'), { id })

export const startLocalServer = (id: string): Promise<void> =>
	invoke(p('local_servers_start'), { id })

export const stopLocalServer = (id: string): Promise<void> =>
	invoke(p('local_servers_stop'), { id })

export const killLocalServer = (id: string): Promise<void> =>
	invoke(p('local_servers_kill'), { id })

export const sendLocalServerCommand = (id: string, command: string): Promise<void> =>
	invoke(p('local_servers_send_command'), { id, command })

export const getLocalServerLogs = (id: string): Promise<string[]> =>
	invoke(p('local_servers_get_logs'), { id })

export const getLocalServerProperties = (id: string): Promise<Record<string, string>> =>
	invoke(p('local_servers_get_properties'), { id })

export const setLocalServerProperties = (
	id: string,
	properties: Record<string, string>,
): Promise<void> => invoke(p('local_servers_set_properties'), { id, properties })

export const updateLocalServerSettings = (
	id: string,
	settings: UpdateLocalServerSettings,
): Promise<LocalServer> => invoke(p('local_servers_update_settings'), { id, settings })

export const getLocalServerGameVersions = (
	loader: LocalServerLoader,
	includeSnapshots = false,
): Promise<string[]> => invoke(p('local_servers_get_game_versions'), { loader, includeSnapshots })

export const getLocalServerLoaderVersions = (
	loader: LocalServerLoader,
	gameVersion: string,
): Promise<string[]> => invoke(p('local_servers_get_loader_versions'), { loader, gameVersion })

export const listLocalServerContent = (id: string): Promise<LocalServerContentFile[]> =>
	invoke(p('local_servers_list_content'), { id })

export const toggleLocalServerContent = (id: string, fileName: string): Promise<string> =>
	invoke(p('local_servers_toggle_content'), { id, fileName })

export const removeLocalServerContent = (id: string, fileName: string): Promise<void> =>
	invoke(p('local_servers_remove_content'), { id, fileName })

export const searchLocalServerContent = (
	id: string,
	query: string,
	offset = 0,
): Promise<LocalServerSearchResult> =>
	invoke(p('local_servers_search_content'), { id, query, offset })

export const installLocalServerProject = (id: string, projectId: string): Promise<string[]> =>
	invoke(p('local_servers_install_project'), { id, projectId })

export const getLocalServerNetworkInfo = (id: string): Promise<LocalServerNetworkInfo> =>
	invoke(p('local_servers_get_network_info'), { id })

export const onLocalServerLog = (
	handler: (event: { id: string; line: string }) => void,
): Promise<UnlistenFn> =>
	listen<{ id: string; line: string }>('local-server://log', (e) => handler(e.payload))

export const onLocalServerStatus = (
	handler: (event: { id: string; status: LocalServerStatus }) => void,
): Promise<UnlistenFn> =>
	listen<{ id: string; status: LocalServerStatus }>('local-server://status', (e) =>
		handler(e.payload),
	)

export const onLocalServerProgress = (
	handler: (event: { id: string; message: string }) => void,
): Promise<UnlistenFn> =>
	listen<{ id: string; message: string }>('local-server://progress', (e) => handler(e.payload))
