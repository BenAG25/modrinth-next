export type LocalServerLoader = 'vanilla' | 'fabric' | 'paper' | 'forge' | 'neoforge'

export type LocalServerStatus = 'stopped' | 'starting' | 'running' | 'stopping'

export interface LocalServerInstalledProject {
	project_id: string
	version_id: string
	file_name: string
	title: string
	icon_url: string | null
	version_number: string
}

export interface LocalServer {
	id: string
	name: string
	loader: LocalServerLoader
	game_version: string
	loader_version: string | null
	memory_mb: number
	java_major: number
	jvm_args: string[]
	created: string
	projects: LocalServerInstalledProject[]
	path: string
	status: LocalServerStatus
	port: number
}

export interface CreateLocalServer {
	name: string
	loader: LocalServerLoader
	game_version: string
	loader_version: string | null
	memory_mb: number
	port: number
	accept_eula: boolean
	motd?: string | null
}

export interface UpdateLocalServerSettings {
	name?: string
	memory_mb?: number
	jvm_args?: string[]
}

export interface LocalServerContentFile {
	file_name: string
	enabled: boolean
	size: number
	project: LocalServerInstalledProject | null
}

export interface LocalServerNetworkInfo {
	port: number
	local_address: string
	lan_address: string | null
}

export interface LocalServerSearchHit {
	project_id: string
	slug: string
	title: string
	description: string
	icon_url: string | null
	downloads: number
	author: string
}

export interface LocalServerSearchResult {
	hits: LocalServerSearchHit[]
	offset: number
	limit: number
	total_hits: number
}
