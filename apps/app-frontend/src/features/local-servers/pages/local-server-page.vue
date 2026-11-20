<script setup lang="ts">
import {
	CopyIcon,
	FolderOpenIcon,
	GlobeIcon,
	MoreVerticalIcon,
	PlayIcon,
	ServerIcon,
	SkullIcon,
	SpinnerIcon,
	StopCircleIcon,
} from '@modrinth/assets'
import {
	Button,
	type ButtonMenuOption,
	injectNotificationManager,
	NavTabs,
	PageHeader,
	PageHeaderActions,
	PageHeaderBadgeItem,
	PageHeaderMetadata,
	PageHeaderMetadataItem,
	TagIcon,
	TeleportOverflowMenu,
} from '@modrinth/ui'
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

import { contentLabel, loaderLabel, STATUS_BADGE_CLASSES, STATUS_LABELS } from '../loaders'
import ProfileLinkCard from '../profile-link-card.vue'
import ServerConsole from '../server-console.vue'
import ServerContentTab from '../server-content-tab.vue'
import ServerSettingsTab from '../server-settings-tab.vue'

defineOptions({ name: 'LocalServerPage' })

const route = useRoute()
const router = useRouter()
const { handleError, addNotification } = injectNotificationManager()

const id = computed(() => String(route.params.id ?? ''))
const tab = computed(() => {
	const t = route.query.tab
	return t === 'content' || t === 'settings' ? t : 'console'
})
const server = ref<LocalServer | null>(null)
const network = ref<LocalServerNetworkInfo | null>(null)
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
	{ label: 'Konsole', href: '' },
	{
		label: server.value ? contentLabel(server.value.loader) : 'Mods',
		href: 'content',
	},
	{ label: 'Einstellungen', href: 'settings' },
])

const moreOptions = computed<ButtonMenuOption[]>(() => [
	{
		id: 'open-folder',
		label: 'Ordner öffnen',
		icon: FolderOpenIcon,
		action: () => server.value && void openPath(server.value.path),
	},
	{
		id: 'kill',
		label: 'Beenden erzwingen',
		icon: SkullIcon,
		tone: 'red',
		shown: !!server.value && server.value.status !== 'stopped',
		action: () => void kill(),
	},
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
	router.replace({ query: {} })
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
	addNotification({ type: 'success', title: 'Adresse kopiert', text })
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
	<div v-if="server" class="flex h-full flex-col">
		<div class="p-6 pb-4">
			<PageHeader :title="server.name">
				<template #leading>
					<div
						class="grid size-16 shrink-0 place-items-center rounded-2xl border border-solid border-surface-4 bg-bg-raised"
					>
						<ServerIcon class="size-8 text-secondary" />
					</div>
				</template>

				<template #badges>
					<PageHeaderBadgeItem :class="STATUS_BADGE_CLASSES[server.status]">
						{{ STATUS_LABELS[server.status] }}
					</PageHeaderBadgeItem>
				</template>

				<template #metadata>
					<PageHeaderMetadata>
						<PageHeaderMetadataItem
							:icon="TagIcon"
							:icon-props="{
								tag: server.loader === 'paper' ? 'paper' : server.loader,
								enforceType: 'loader',
							}"
							tooltip="Server-Software und Minecraft-Version"
						>
							{{ loaderLabel(server.loader) }} {{ server.game_version }}
						</PageHeaderMetadataItem>
						<PageHeaderMetadataItem
							v-if="network"
							:icon="CopyIcon"
							tooltip="Adresse für dich kopieren"
							:action="() => copy(network!.local_address)"
						>
							{{ network.local_address }}
						</PageHeaderMetadataItem>
						<PageHeaderMetadataItem
							v-if="network?.lan_address"
							:icon="GlobeIcon"
							tooltip="Adresse für Freunde im selben Netzwerk kopieren"
							:action="() => copy(network!.lan_address!)"
						>
							{{ network.lan_address }}
						</PageHeaderMetadataItem>
					</PageHeaderMetadata>
				</template>

				<template #actions>
					<PageHeaderActions>
						<Button
							v-if="server.status === 'stopped'"
							type="colored"
							color="brand"
							size="xl"
							:disabled="busy"
							@click="start"
						>
							<SpinnerIcon v-if="busy" class="animate-spin" />
							<PlayIcon v-else />
							Server starten
						</Button>
						<Button
							v-else
							type="colored"
							color="red"
							size="xl"
							:disabled="busy || server.status === 'stopping'"
							@click="stop"
						>
							<StopCircleIcon />
							{{ server.status === 'stopping' ? 'Stoppt…' : 'Server stoppen' }}
						</Button>
						<TeleportOverflowMenu
							type="quiet"
							size="xl"
							label="Weitere Aktionen"
							tooltip="Weitere Aktionen"
							:options="moreOptions"
						>
							<MoreVerticalIcon />
						</TeleportOverflowMenu>
					</PageHeaderActions>
				</template>
			</PageHeader>
		</div>

		<div class="flex min-h-0 flex-1 flex-col gap-4 px-6 pb-6">
			<ProfileLinkCard :server="server" @changed="load" />
			<NavTabs :links="tabs" query="tab" replace />
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
	</div>
	<div v-else class="flex items-center gap-2 p-6 text-secondary">
		<SpinnerIcon class="animate-spin" /> Lädt…
	</div>
</template>
