<script setup lang="ts">
import {
	BoxIcon,
	CheckIcon,
	DownloadIcon,
	PlusIcon,
	SearchIcon,
	SpinnerIcon,
	TrashIcon,
	XIcon,
} from '@modrinth/assets'
import { Avatar, Button, injectNotificationManager, Toggle } from '@modrinth/ui'
import { computed, onMounted, ref, watch } from 'vue'

import {
	installLocalServerProject,
	listLocalServerContent,
	removeLocalServerContent,
	searchLocalServerContent,
	toggleLocalServerContent,
} from '@/platform/app-lib/local-servers/commands'
import type {
	LocalServer,
	LocalServerContentFile,
	LocalServerSearchHit,
} from '@/platform/app-lib/local-servers/types'

import { contentLabel } from './loaders'

const props = defineProps<{ server: LocalServer }>()
const emit = defineEmits<{ changed: [] }>()
const { handleError, addNotification } = injectNotificationManager()

const files = ref<LocalServerContentFile[]>([])
const loadingFiles = ref(true)
const browsing = ref(false)

const query = ref('')
const results = ref<LocalServerSearchHit[]>([])
const totalHits = ref(0)
const searching = ref(false)
const installing = ref<Record<string, boolean>>({})

const label = computed(() => contentLabel(props.server.loader))
const installedProjectIds = computed(() => new Set(props.server.projects.map((p) => p.project_id)))
const filter = ref('')
const filteredFiles = computed(() => {
	const f = filter.value.trim().toLowerCase()
	if (!f) return files.value
	return files.value.filter((file) =>
		(file.project?.title ?? file.file_name).toLowerCase().includes(f),
	)
})

async function loadFiles() {
	try {
		files.value = await listLocalServerContent(props.server.id)
	} catch (e) {
		handleError(e as Error)
	} finally {
		loadingFiles.value = false
	}
}

let searchTimer: ReturnType<typeof setTimeout> | undefined
async function search(append = false) {
	searching.value = true
	try {
		const res = await searchLocalServerContent(
			props.server.id,
			query.value,
			append ? results.value.length : 0,
		)
		results.value = append ? [...results.value, ...res.hits] : res.hits
		totalHits.value = res.total_hits
	} catch (e) {
		handleError(e as Error)
	} finally {
		searching.value = false
	}
}
watch(query, () => {
	clearTimeout(searchTimer)
	searchTimer = setTimeout(() => search(), 300)
})

function openBrowser() {
	browsing.value = true
	if (results.value.length === 0) search()
}

async function install(hit: LocalServerSearchHit) {
	installing.value[hit.project_id] = true
	try {
		const installed = await installLocalServerProject(props.server.id, hit.project_id)
		addNotification({
			type: 'success',
			title: `${hit.title} installiert`,
			text:
				installed.length > 1
					? `Inklusive Abhängigkeiten: ${installed.filter((t) => t !== hit.title).join(', ')}`
					: undefined,
		})
		emit('changed')
		await loadFiles()
	} catch (e) {
		handleError(e as Error)
	} finally {
		installing.value[hit.project_id] = false
	}
}

async function toggle(file: LocalServerContentFile) {
	try {
		await toggleLocalServerContent(props.server.id, file.file_name)
		await loadFiles()
	} catch (e) {
		handleError(e as Error)
	}
}

async function remove(file: LocalServerContentFile) {
	try {
		await removeLocalServerContent(props.server.id, file.file_name)
		emit('changed')
		await loadFiles()
	} catch (e) {
		handleError(e as Error)
	}
}

function formatDownloads(n: number) {
	return Intl.NumberFormat(undefined, { notation: 'compact' }).format(n)
}

onMounted(loadFiles)
watch(() => props.server.id, loadFiles)
</script>

<template>
	<div class="flex flex-col gap-4">
		<div
			v-if="server.status !== 'stopped'"
			class="rounded-xl bg-bg-raised px-4 py-3 text-sm text-secondary"
		>
			Änderungen an {{ label }} werden nach einem Neustart des Servers aktiv.
		</div>

		<!-- Browse Modrinth -->
		<div v-if="browsing" class="flex flex-col gap-3 rounded-2xl bg-bg-raised p-4">
			<div class="flex items-center gap-2">
				<div class="relative flex-1">
					<SearchIcon class="absolute left-3 top-1/2 -translate-y-1/2 text-secondary" />
					<input
						v-model="query"
						type="text"
						class="w-full !pl-10"
						:placeholder="`${label} auf Modrinth suchen (${server.game_version})…`"
						autofocus
					/>
				</div>
				<Button type="outlined" @click="browsing = false"><XIcon /> Schließen</Button>
			</div>
			<div v-if="searching && results.length === 0" class="flex items-center gap-2 text-secondary">
				<SpinnerIcon class="animate-spin" /> Suche…
			</div>
			<div v-else-if="results.length === 0" class="text-secondary">
				Keine passenden {{ label }} für {{ server.game_version }} gefunden.
			</div>
			<div class="flex flex-col gap-2">
				<div
					v-for="hit in results"
					:key="hit.project_id"
					class="flex items-center gap-3 rounded-xl bg-bg p-3"
				>
					<Avatar :src="hit.icon_url ?? undefined" size="3rem" />
					<div class="min-w-0 flex-1">
						<div class="flex items-baseline gap-2">
							<span class="truncate font-bold text-contrast">{{ hit.title }}</span>
							<span class="shrink-0 text-xs text-secondary">von {{ hit.author }}</span>
						</div>
						<div class="line-clamp-2 text-sm text-secondary">{{ hit.description }}</div>
						<div class="mt-1 flex items-center gap-1 text-xs text-secondary">
							<DownloadIcon class="h-3 w-3" /> {{ formatDownloads(hit.downloads) }}
						</div>
					</div>
					<Button
						v-if="installedProjectIds.has(hit.project_id)"
						type="outlined"
						size="sm"
						:disabled="installing[hit.project_id]"
						@click="install(hit)"
					>
						<CheckIcon /> Installiert
					</Button>
					<Button
						v-else
						type="colored"
						color="brand"
						size="sm"
						:disabled="installing[hit.project_id]"
						@click="install(hit)"
					>
						<SpinnerIcon v-if="installing[hit.project_id]" class="animate-spin" />
						<DownloadIcon v-else />
						Installieren
					</Button>
				</div>
			</div>
			<Button
				v-if="results.length > 0 && results.length < totalHits"
				class="self-center"
				:disabled="searching"
				@click="search(true)"
			>
				Mehr laden
			</Button>
		</div>

		<!-- Installed -->
		<div class="flex flex-wrap items-center justify-between gap-2">
			<h2 class="m-0 text-lg font-bold text-contrast">
				Installierte {{ label }} ({{ files.length }})
			</h2>
			<div class="flex items-center gap-2">
				<input v-if="files.length > 5" v-model="filter" type="text" placeholder="Filtern…" />
				<Button v-if="!browsing" type="colored" color="brand" @click="openBrowser">
					<PlusIcon /> {{ label }} hinzufügen
				</Button>
			</div>
		</div>

		<div v-if="loadingFiles" class="flex items-center gap-2 text-secondary">
			<SpinnerIcon class="animate-spin" /> Lädt…
		</div>
		<div
			v-else-if="files.length === 0"
			class="flex flex-col items-center gap-2 rounded-2xl bg-bg-raised p-8 text-center text-secondary"
		>
			<BoxIcon class="h-10 w-10" />
			Noch keine {{ label }} installiert.
		</div>
		<div v-else class="flex flex-col gap-2">
			<div
				v-for="file in filteredFiles"
				:key="file.file_name"
				class="flex items-center gap-3 rounded-xl bg-bg-raised p-3"
				:class="{ 'opacity-60': !file.enabled }"
			>
				<Avatar :src="file.project?.icon_url ?? undefined" size="2.5rem" />
				<div class="min-w-0 flex-1">
					<div class="truncate font-bold text-contrast">
						{{ file.project?.title ?? file.file_name }}
					</div>
					<div class="truncate text-xs text-secondary">
						<template v-if="file.project">{{ file.project.version_number }} · </template>
						{{ file.file_name }}
					</div>
				</div>
				<Toggle
					:model-value="file.enabled"
					:aria-label="file.enabled ? 'Deaktivieren' : 'Aktivieren'"
					@update:model-value="toggle(file)"
				/>
				<Button type="quiet" color="red" size="sm" @click="remove(file)">
					<TrashIcon />
				</Button>
			</div>
		</div>
	</div>
</template>
