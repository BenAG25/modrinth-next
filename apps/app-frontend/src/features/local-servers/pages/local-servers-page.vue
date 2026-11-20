<script setup lang="ts">
import { PlayIcon, PlusIcon, ServerIcon, SpinnerIcon, StopCircleIcon } from '@modrinth/assets'
import { Button, injectNotificationManager } from '@modrinth/ui'
import { onActivated, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import {
	listLocalServers,
	onLocalServerStatus,
	startLocalServer,
	stopLocalServer,
} from '@/platform/app-lib/local-servers/commands'
import type { LocalServer } from '@/platform/app-lib/local-servers/types'
import { useRootBreadcrumb } from '@/shared/breadcrumbs'

import CreateLocalServerModal from '../create-local-server-modal.vue'
import { contentLabel, loaderLabel, STATUS_COLORS, STATUS_LABELS } from '../loaders'

defineOptions({ name: 'LocalServersPage' })

const router = useRouter()
const { handleError } = injectNotificationManager()

const breadcrumb = useRootBreadcrumb({
	slot: 'root',
	id: 'local-servers',
	label: 'Lokale Server',
	to: '/local-servers',
	visual: { type: 'icon', component: ServerIcon },
})

const servers = ref<LocalServer[]>([])
const loading = ref(true)
const busy = ref<Record<string, boolean>>({})
const createModal = ref<InstanceType<typeof CreateLocalServerModal> | null>(null)
let unlisten: (() => void) | null = null

async function refresh() {
	try {
		servers.value = await listLocalServers()
	} catch (e) {
		handleError(e as Error)
	} finally {
		loading.value = false
	}
}

async function toggle(server: LocalServer) {
	busy.value[server.id] = true
	try {
		if (server.status === 'stopped') await startLocalServer(server.id)
		else await stopLocalServer(server.id)
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value[server.id] = false
	}
}

function onCreated(server: LocalServer) {
	router.push(`/local-servers/${server.id}`)
}

onMounted(async () => {
	unlisten = await onLocalServerStatus(({ id, status }) => {
		const s = servers.value.find((x) => x.id === id)
		if (s) s.status = status
	})
	await refresh()
})
onActivated(() => {
	breadcrumb.reset()
	refresh()
})
onBeforeUnmount(() => unlisten?.())
</script>

<template>
	<div class="box-border flex h-full flex-col gap-6 p-6">
		<div class="flex flex-wrap items-center justify-between gap-4">
			<div>
				<h1 class="m-0 text-2xl font-extrabold text-contrast">Lokale Server</h1>
				<p class="m-0 mt-1 text-secondary">
					Erstelle Minecraft-Server auf deinem PC, installiere Mods mit einem Klick und spiele mit
					Freunden im selben Netzwerk.
				</p>
			</div>
			<Button type="colored" color="brand" size="lg" @click="createModal?.show()">
				<PlusIcon /> Server erstellen
			</Button>
		</div>

		<div v-if="loading" class="flex items-center gap-2 text-secondary">
			<SpinnerIcon class="animate-spin" /> Lädt…
		</div>

		<div
			v-else-if="servers.length === 0"
			class="flex flex-col items-center gap-4 rounded-2xl bg-bg-raised p-12 text-center"
		>
			<ServerIcon class="h-12 w-12 text-secondary" />
			<div>
				<h2 class="m-0 text-xl font-bold text-contrast">Noch keine lokalen Server</h2>
				<p class="m-0 mt-1 text-secondary">
					Wähle Fabric, NeoForge, Forge, Paper oder Vanilla – der Rest wird automatisch
					eingerichtet.
				</p>
			</div>
			<Button type="colored" color="brand" @click="createModal?.show()">
				<PlusIcon /> Ersten Server erstellen
			</Button>
		</div>

		<div v-else class="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-4">
			<div
				v-for="server in servers"
				:key="server.id"
				role="button"
				tabindex="0"
				class="flex cursor-pointer flex-col gap-3 rounded-2xl bg-bg-raised p-4 transition hover:brightness-110"
				@click="router.push(`/local-servers/${server.id}`)"
				@keydown.enter="router.push(`/local-servers/${server.id}`)"
			>
				<div class="flex items-start justify-between gap-2">
					<div class="flex min-w-0 items-center gap-3">
						<div class="grid h-12 w-12 shrink-0 place-items-center rounded-xl bg-button-bg">
							<ServerIcon class="h-6 w-6 text-secondary" />
						</div>
						<div class="min-w-0">
							<div class="truncate text-lg font-bold text-contrast">{{ server.name }}</div>
							<div class="text-sm text-secondary">
								{{ loaderLabel(server.loader) }} {{ server.game_version }}
							</div>
						</div>
					</div>
					<div class="flex items-center gap-1.5 text-sm font-semibold">
						<span class="h-2.5 w-2.5 rounded-full" :class="STATUS_COLORS[server.status]" />
						{{ STATUS_LABELS[server.status] }}
					</div>
				</div>
				<div class="flex items-center justify-between gap-2 text-sm text-secondary">
					<span>
						{{ server.projects.length }} {{ contentLabel(server.loader) }} ·
						{{ (server.memory_mb / 1024).toFixed(1).replace('.0', '') }} GB · Port {{ server.port }}
					</span>
					<Button
						:type="server.status === 'stopped' ? 'colored' : 'outlined'"
						:color="server.status === 'stopped' ? 'brand' : 'red'"
						size="sm"
						:disabled="busy[server.id] || server.status === 'stopping'"
						@click.stop="toggle(server)"
					>
						<template v-if="server.status === 'stopped'"><PlayIcon /> Starten</template>
						<template v-else><StopCircleIcon /> Stoppen</template>
					</Button>
				</div>
			</div>
		</div>

		<CreateLocalServerModal ref="createModal" @created="onCreated" />
	</div>
</template>
