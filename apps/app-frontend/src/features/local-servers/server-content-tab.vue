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
import { Avatar, Button, injectNotificationManager, Input, Toggle } from '@modrinth/ui'
import { computed, onMounted, ref, watch } from 'vue'

import {
	getLocalServer,
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
import { sharesMods, syncServerToProfile } from './profile-link'

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
const syncsWithProfile = computed(
	() => !!props.server.linked_instance && sharesMods(props.server.loader),
)
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
		const report = await installLocalServerProject(props.server.id, hit.project_id)
		const lines: string[] = []
		const deps = report.installed.filter((t) => t !== hit.title)
		if (deps.length) lines.push(`Abhängigkeiten: ${deps.join(', ')}`)

		if (syncsWithProfile.value && props.server.linked_instance) {
			const updated = await getLocalServer(props.server.id)
			const profileReport = await syncServerToProfile(updated, props.server.linked_instance)
			if (profileReport.toProfile.length) lines.push('Auch im Spielprofil installiert.')
			if (profileReport.serverOnly.length)
				lines.push(`Nur serverseitig: ${profileReport.serverOnly.join(', ')}`)
		}

		addNotification({
			type: 'success',
			title: `${hit.title} installiert`,
			text: lines.length ? lines.join('\n') : undefined,
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
		<!-- Toolbar -->
		<div class="flex flex-wrap items-center gap-2">
			<Input
				v-model="filter"
				:icon="SearchIcon"
				type="search"
				:placeholder="`Installierte ${label} durchsuchen…`"
				wrapper-class="min-w-0 flex-1"
				clearable
			/>
			<Button v-if="!browsing" type="colored" color="brand" @click="openBrowser">
				<PlusIcon /> {{ label }} hinzufügen
			</Button>
		</div>

		<div
			v-if="server.status !== 'stopped'"
			class="rounded-2xl border border-solid border-surface-4 bg-bg-raised px-4 py-3 text-sm text-secondary"
		>
			Änderungen an {{ label }} werden nach einem Neustart des Servers aktiv.
		</div>

		<!-- Browse Modrinth -->
		<div
			v-if="browsing"
			class="flex flex-col gap-3 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-4"
		>
			<div class="flex items-center gap-2">
				<Input
					v-model="query"
					:icon="SearchIcon"
					type="search"
					:placeholder="`${label} auf Modrinth suchen (${server.game_version})…`"
					wrapper-class="min-w-0 flex-1"
					clearable
				/>
				<Button type="quiet" @click="browsing = false"><XIcon /> Schließen</Button>
			</div>
			<div v-if="syncsWithProfile" class="text-sm text-secondary">
				Neue {{ label }} werden auch im verknüpften Spielprofil installiert, wenn der Client sie
				braucht.
			</div>
			<div v-if="searching && results.length === 0" class="flex items-center gap-2 text-secondary">
				<SpinnerIcon class="animate-spin" /> Suche…
			</div>
			<div v-else-if="results.length === 0" class="py-4 text-center text-secondary">
				Keine passenden {{ label }} für {{ server.game_version }} gefunden.
			</div>
			<div v-else class="flex flex-col gap-2">
				<div
					v-for="hit in results"
					:key="hit.project_id"
					class="flex items-center gap-3 rounded-xl bg-surface-2 p-3"
				>
					<Avatar :src="hit.icon_url ?? undefined" size="48px" />
					<div class="min-w-0 flex-1">
						<div class="flex items-baseline gap-2">
							<span class="truncate font-bold text-contrast">{{ hit.title }}</span>
							<span class="shrink-0 text-xs text-secondary">von {{ hit.author }}</span>
						</div>
						<div class="line-clamp-2 text-sm text-secondary">{{ hit.description }}</div>
						<div class="mt-1 flex items-center gap-1 text-xs text-secondary">
							<DownloadIcon class="size-3" /> {{ formatDownloads(hit.downloads) }}
						</div>
					</div>
					<Button
						v-if="installedProjectIds.has(hit.project_id)"
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
		<div v-if="loadingFiles" class="flex items-center gap-2 text-secondary">
			<SpinnerIcon class="animate-spin" /> Lädt…
		</div>
		<div
			v-else-if="files.length === 0"
			class="flex flex-col items-center gap-3 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-10 text-center"
		>
			<BoxIcon class="size-10 text-secondary" />
			<div class="text-secondary">Noch keine {{ label }} installiert.</div>
			<Button v-if="!browsing" type="colored" color="brand" @click="openBrowser">
				<PlusIcon /> {{ label }} hinzufügen
			</Button>
		</div>
		<div
			v-else
			class="overflow-hidden rounded-2xl border border-solid border-surface-4 bg-bg-raised"
		>
			<div
				class="flex items-center justify-between border-0 border-b border-solid border-surface-4 px-4 py-3 text-sm font-semibold text-secondary"
			>
				<span>{{ files.length }} {{ label }}</span>
				<span>Aktiv</span>
			</div>
			<div
				v-for="(file, index) in filteredFiles"
				:key="file.file_name"
				class="flex items-center gap-3 px-4 py-3"
				:class="{
					'border-0 border-t border-solid border-surface-4': index > 0,
					'opacity-60': !file.enabled,
				}"
			>
				<Avatar :src="file.project?.icon_url ?? undefined" size="40px" />
				<div class="min-w-0 flex-1">
					<div class="truncate font-bold text-contrast">
						{{ file.project?.title ?? file.file_name }}
					</div>
					<div class="truncate text-xs text-secondary">
						<template v-if="file.project">{{ file.project.version_number }} · </template>
						{{ file.file_name }}
					</div>
				</div>
				<Button v-tooltip="'Entfernen'" type="quiet" color="red" size="sm" @click="remove(file)">
					<TrashIcon />
				</Button>
				<Toggle :model-value="file.enabled" @update:model-value="toggle(file)" />
			</div>
		</div>
	</div>
</template>
