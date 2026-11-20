<script setup lang="ts">
import {
	ArrowLeftIcon,
	CopyIcon,
	FolderOpenIcon,
	PlayIcon,
	PowerIcon,
	ServerIcon,
	SkullIcon,
	SpinnerIcon,
	StopCircleIcon,
} from '@modrinth/assets'
import { Button, injectNotificationManager } from '@modrinth/ui'
import { computed, onActivated, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { openPath } from '@/helpers/utils.js'
import {
	getLocalServer,
	getLocalServerNetworkInfo,
	killLocalServer,
	onLocalServerStatus,
	startLocalServer,
	stopLocalServer,
} from '@/platform/app-lib/local-servers/commands'
import type { LocalServer, LocalServerNetworkInfo } from '@/platform/app-lib/local-servers/types'
import { useBreadcrumb, useRootBreadcrumb } from '@/shared/breadcrumbs'

import { contentLabel, loaderLabel, STATUS_COLORS, STATUS_LABELS } from '../loaders'
import ServerConsole from '../server-console.vue'
import ServerContentTab from '../server-content-tab.vue'
import ServerSettingsTab from '../server-settings-tab.vue'

defineOptions({ name: 'LocalServerPage' })

const route = useRoute()
const router = useRouter()
const { handleError, addNotification } = injectNotificationManager()

const id = computed(() => String(route.params.id ?? ''))
const server = ref<LocalServer | null>(null)
const network = ref<LocalServerNetworkInfo | null>(null)
const tab = ref<'console' | 'content' | 'settings'>('console')
const busy = ref(false)
let unlisten: (() => void) | null = null

const rootBreadcrumb = useRootBreadcrumb({
	slot: 'root',
	id: 'local-servers',
	label: 'Lokale Server',
	to: '/local-servers',
	visual: { type: 'icon', component: ServerIcon },
})
const serverBreadcrumb = useBreadcrumb(
	{
		slot: 'local-server',
		id: () => `local-server:${id.value}`,
		label: () => server.value?.name ?? 'Server',
		to: () => `/local-servers/${id.value}`,
	},
	{ parent: rootBreadcrumb },
)

const tabs = computed(() => [
	{ id: 'console' as const, label: 'Konsole' },
	{ id: 'content' as const, label: server.value ? contentLabel(server.value.loader) : 'Mods' },
	{ id: 'settings' as const, label: 'Einstellungen' },
])

async function load() {
	if (!id.value) return
	try {
		server.value = await getLocalServer(id.value)
		network.value = await getLocalServerNetworkInfo(id.value)
	} catch (e) {
		handleError(e as Error)
		router.replace('/local-servers')
	}
}

async function start() {
	if (!server.value) return
	busy.value = true
	tab.value = 'console'
	try {
		await startLocalServer(server.value.id)
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = false
	}
}

async function stop() {
	if (!server.value) return
	busy.value = true
	try {
		await stopLocalServer(server.value.id)
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = false
	}
}

async function kill() {
	if (!server.value) return
	try {
		await killLocalServer(server.value.id)
	} catch (e) {
		handleError(e as Error)
	}
}

async function copy(text: string) {
	await navigator.clipboard.writeText(text)
	addNotification({ type: 'success', title: 'Kopiert', text })
}

function onUpdated(updated: LocalServer) {
	server.value = updated
	getLocalServerNetworkInfo(updated.id).then((n) => (network.value = n))
}

onMounted(async () => {
	unlisten = await onLocalServerStatus(({ id: changed, status }) => {
		if (server.value && changed === server.value.id) server.value.status = status
	})
	await load()
})
onActivated(() => {
	rootBreadcrumb.reset()
	serverBreadcrumb.activate()
	load()
})
onBeforeUnmount(() => unlisten?.())
watch(id, load)
</script>

<template>
	<div v-if="server" class="box-border flex h-full flex-col gap-5 p-6">
		<div class="flex flex-wrap items-center justify-between gap-4">
			<div class="flex min-w-0 items-center gap-4">
				<Button type="quiet" size="sm" @click="router.push('/local-servers')">
					<ArrowLeftIcon />
				</Button>
				<div class="grid h-14 w-14 shrink-0 place-items-center rounded-2xl bg-bg-raised">
					<ServerIcon class="h-7 w-7 text-secondary" />
				</div>
				<div class="min-w-0">
					<h1 class="m-0 truncate text-2xl font-extrabold text-contrast">{{ server.name }}</h1>
					<div class="flex flex-wrap items-center gap-2 text-sm text-secondary">
						<span>{{ loaderLabel(server.loader) }} {{ server.game_version }}</span>
						<span v-if="server.loader_version">· {{ server.loader_version }}</span>
						<span>·</span>
						<span class="flex items-center gap-1.5 font-semibold">
							<span class="h-2.5 w-2.5 rounded-full" :class="STATUS_COLORS[server.status]" />
							{{ STATUS_LABELS[server.status] }}
						</span>
					</div>
				</div>
			</div>
			<div class="flex flex-wrap items-center gap-2">
				<Button type="outlined" @click="openPath(server.path)"> <FolderOpenIcon /> Ordner </Button>
				<Button
					v-if="server.status === 'stopped'"
					type="colored"
					color="brand"
					size="lg"
					:disabled="busy"
					@click="start"
				>
					<SpinnerIcon v-if="busy" class="animate-spin" />
					<PlayIcon v-else />
					Starten
				</Button>
				<template v-else>
					<Button
						type="outlined"
						color="red"
						size="lg"
						:disabled="busy || server.status === 'stopping'"
						@click="stop"
					>
						<StopCircleIcon /> Stoppen
					</Button>
					<Button
						v-if="server.status === 'stopping' || server.status === 'starting'"
						type="quiet"
						color="red"
						size="lg"
						title="Prozess sofort beenden"
						@click="kill"
					>
						<SkullIcon /> Beenden erzwingen
					</Button>
				</template>
			</div>
		</div>

		<div v-if="network" class="flex flex-wrap gap-3">
			<button
				type="button"
				class="flex items-center gap-2 rounded-xl bg-bg-raised px-4 py-2 text-left hover:brightness-110"
				@click="copy(network.local_address)"
			>
				<PowerIcon class="text-secondary" />
				<span>
					<span class="block text-xs text-secondary">Auf diesem PC</span>
					<span class="font-mono font-semibold text-contrast">{{ network.local_address }}</span>
				</span>
				<CopyIcon class="text-secondary" />
			</button>
			<button
				v-if="network.lan_address"
				type="button"
				class="flex items-center gap-2 rounded-xl bg-bg-raised px-4 py-2 text-left hover:brightness-110"
				@click="copy(network.lan_address)"
			>
				<ServerIcon class="text-secondary" />
				<span>
					<span class="block text-xs text-secondary">Im Heimnetzwerk (LAN)</span>
					<span class="font-mono font-semibold text-contrast">{{ network.lan_address }}</span>
				</span>
				<CopyIcon class="text-secondary" />
			</button>
		</div>

		<div class="flex gap-1 rounded-full bg-bg-raised p-1 w-fit">
			<button
				v-for="t in tabs"
				:key="t.id"
				type="button"
				class="rounded-full px-4 py-1.5 font-semibold transition-colors"
				:class="tab === t.id ? 'bg-button-bg text-contrast' : 'text-secondary hover:text-contrast'"
				@click="tab = t.id"
			>
				{{ t.label }}
				<span v-if="t.id === 'content'" class="text-secondary">({{ server.projects.length }})</span>
			</button>
		</div>

		<div class="min-h-0 flex-1">
			<ServerConsole v-show="tab === 'console'" :server-id="server.id" :status="server.status" />
			<ServerContentTab v-if="tab === 'content'" :server="server" @changed="load" />
			<ServerSettingsTab
				v-if="tab === 'settings'"
				:server="server"
				@updated="onUpdated"
				@deleted="router.replace('/local-servers')"
			/>
		</div>
	</div>
	<div v-else class="flex items-center gap-2 p-6 text-secondary">
		<SpinnerIcon class="animate-spin" /> Lädt…
	</div>
</template>
