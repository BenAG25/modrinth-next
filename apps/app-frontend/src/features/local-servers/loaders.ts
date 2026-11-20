import type { LocalServerLoader, LocalServerStatus } from '@/platform/app-lib/local-servers/types'

export const LOADERS: { id: LocalServerLoader; label: string; description: string }[] = [
	{
		id: 'fabric',
		label: 'Fabric',
		description: 'Leicht und schnell. Die meisten modernen Mods auf Modrinth laufen mit Fabric.',
	},
	{
		id: 'neoforge',
		label: 'NeoForge',
		description: 'Für große Mods und Modpacks ab Minecraft 1.20.2.',
	},
	{
		id: 'forge',
		label: 'Forge',
		description: 'Der Klassiker für Mods, vor allem für ältere Minecraft-Versionen.',
	},
	{
		id: 'paper',
		label: 'Paper',
		description:
			'Performanter Server mit Plugins (Bukkit/Spigot/Paper). Spieler brauchen keine Mods.',
	},
	{
		id: 'vanilla',
		label: 'Vanilla',
		description: 'Der offizielle Minecraft-Server ohne Mods. Datapacks werden unterstützt.',
	},
]

export function loaderLabel(loader: LocalServerLoader): string {
	return LOADERS.find((l) => l.id === loader)?.label ?? loader
}

export function contentLabel(loader: LocalServerLoader): string {
	if (loader === 'paper') return 'Plugins'
	if (loader === 'vanilla') return 'Datapacks'
	return 'Mods'
}

export const STATUS_LABELS: Record<LocalServerStatus, string> = {
	stopped: 'Gestoppt',
	starting: 'Startet…',
	running: 'Online',
	stopping: 'Stoppt…',
}

export const STATUS_COLORS: Record<LocalServerStatus, string> = {
	stopped: 'bg-[var(--color-gray,#888)]',
	starting: 'bg-orange',
	running: 'bg-green',
	stopping: 'bg-orange',
}

export const STATUS_BADGE_CLASSES: Record<LocalServerStatus, string> = {
	stopped: '!border-surface-5 !bg-surface-3 !text-secondary',
	starting: '!border-orange !bg-highlight-orange !text-orange',
	running: '!border-green !bg-highlight-green !text-green',
	stopping: '!border-orange !bg-highlight-orange !text-orange',
}
