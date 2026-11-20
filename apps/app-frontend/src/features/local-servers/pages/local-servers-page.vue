<script setup lang="ts">
import { PlayIcon, PlusIcon, ServerIcon, SpinnerIcon, StopCircleIcon } from '@modrinth/assets'
import { Button, injectNotificationManager } from '@modrinth/ui'
import { nextTick, onActivated, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import {
	listLocalServers,
	onLocalServerStatus,
	startLocalServer,
	stopLocalServer,
} from '@/platform/app-lib/local-servers/commands'
import type { LocalServer } from '@/platform/app-lib/local-servers/types'
import { useRootBreadcrumb } from '@/shared/breadcrumbs'

import CreateLocalServerModal from '../create-local-server-modal.vue'
import { contentLabel, loaderLabel, STATUS_BADGE_CLASSES, STATUS_LABELS } from '../loaders'

defineOptions({ name: 'LocalServersPage' })

const route = useRoute()
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

/** `/local-servers?fromInstance=<id>` opens the create dialog for that profile. */
async function handleFromInstanceQuery() {
	const fromInstance = route.query.fromInstance
	if (typeof fromInstance !== 'string' || !fromInstance) return
	await router.replace({ query: {} })
	await nextTick()
	createModal.value?.show({ fromInstance })
}
watch(() => route.query.fromInstance, handleFromInstanceQuery)

onMounted(async () => {
	unlisten = await onLocalServerStatus(({ id, status }) => {
		const s = servers.value.find((x) => x.id === id)
		if (s) s.status = status
	})
	await refresh()
	await handleFromInstanceQuery()
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
			<div class="min-w-0">
				<h1 class="m-0 text-2xl font-extrabold text-contrast">Lokale Server</h1>
				<p class="m-0 mt-1 text-secondary">
					Minecraft-Server auf deinem PC – mit passendem Spielprofil und denselben Mods.
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
			class="flex flex-col items-center gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-12 text-center"
		>
			<div class="grid size-16 place-items-center rounded-2xl bg-surface-4">
				<ServerIcon class="size-8 text-secondary" />
			</div>
			<div>
				<h2 class="m-0 text-xl font-bold text-contrast">Noch keine lokalen Server</h2>
				<p class="m-0 mt-1 max-w-md text-secondary">
					Wähle Fabric, NeoForge, Forge, Paper oder Vanilla – oder erstelle einen Server direkt aus
					einem deiner Spielprofile.
				</p>
			</div>
			<Button type="colored" color="brand" @click="createModal?.show()">
				<PlusIcon /> Ersten Server erstellen
			</Button>
		</div>

		<div v-else class="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
			<div
				v-for="server in servers"
				:key="server.id"
				role="button"
				tabindex="0"
				class="flex cursor-pointer flex-col gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-4 transition-colors hover:bg-surface-3"
				@click="router.push(`/local-servers/${server.id}`)"
				@keydown.enter="router.push(`/local-servers/${server.id}`)"
			>
				<div class="flex items-start gap-3">
					<div class="grid size-12 shrink-0 place-items-center rounded-xl bg-surface-4">
						<ServerIcon class="size-6 text-secondary" />
					</div>
					<div class="min-w-0 flex-1">
						<div class="truncate text-lg font-bold text-contrast">{{ server.name }}</div>
						<div class="text-sm text-secondary">
							{{ loaderLabel(server.loader) }} {{ server.game_version }}
						</div>
					</div>
					<span
						class="shrink-0 rounded-full border border-solid px-2.5 py-0.5 text-xs font-bold"
						:class="STATUS_BADGE_CLASSES[server.status]"
					>
						{{ STATUS_LABELS[server.status] }}
					</span>
				</div>
				<div class="flex items-center justify-between gap-2">
					<span class="text-sm text-secondary">
						{{ server.projects.length }} {{ contentLabel(server.loader) }} · Port
						{{ server.port }}
						<template v-if="server.linked_instance"> · Profil verknüpft</template>
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
