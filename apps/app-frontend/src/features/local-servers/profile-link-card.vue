<script setup lang="ts">
import {
	GameIcon,
	LinkIcon,
	PlayIcon,
	PlusIcon,
	RefreshCwIcon,
	SpinnerIcon,
	UnlinkIcon,
	XIcon,
} from '@modrinth/assets'
import { Avatar, Button, Combobox, injectNotificationManager } from '@modrinth/ui'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import { getInstanceIconUrl, list } from '@/helpers/instance'
import type { GameInstance } from '@/helpers/types'
import { linkLocalServerInstance } from '@/platform/app-lib/local-servers/commands'
import type { LocalServer } from '@/platform/app-lib/local-servers/types'
import { injectAppEvents } from '@/providers/app-events'

import {
	createProfileForServer,
	describeSync,
	getLinkedProfile,
	playOnServer,
	profileLoaderFor,
	sharesMods,
	syncProfileAndServer,
} from './profile-link'

const props = defineProps<{ server: LocalServer }>()
const emit = defineEmits<{ changed: [] }>()

const router = useRouter()
const appEvents = injectAppEvents()
const { handleError, addNotification } = injectNotificationManager()

const profile = ref<GameInstance | null>(null)
const loading = ref(true)
const working = ref<'create' | 'sync' | 'play' | 'link' | null>(null)
const choosing = ref(false)
const candidates = ref<GameInstance[]>([])
const selectedId = ref<string | undefined>()

const expectedLoader = computed(() => profileLoaderFor(props.server.loader))
const candidateOptions = computed(() =>
	candidates.value.map((i) => ({
		value: i.id,
		label: i.name,
		subLabel: `${i.loader} ${i.game_version}`,
	})),
)
const mismatch = computed(
	() =>
		!!profile.value &&
		(profile.value.game_version !== props.server.game_version ||
			profile.value.loader !== expectedLoader.value),
)

async function loadProfile() {
	loading.value = true
	profile.value = await getLinkedProfile(props.server)
	loading.value = false
}
watch(() => props.server.linked_instance, loadProfile, { immediate: true })

async function createProfile() {
	working.value = 'create'
	try {
		const { report } = await createProfileForServer(props.server, appEvents)
		emit('changed')
		addNotification({
			type: 'success',
			title: 'Spielprofil erstellt',
			text: sharesMods(props.server.loader) ? describeSync(report) : undefined,
		})
	} catch (e) {
		handleError(e as Error)
	} finally {
		working.value = null
	}
}

async function openChooser() {
	choosing.value = true
	const all = await list()
	candidates.value = all.filter(
		(i) => i.game_version === props.server.game_version && i.loader === expectedLoader.value,
	)
	selectedId.value = candidates.value[0]?.id
}

async function link() {
	if (!selectedId.value) return
	working.value = 'link'
	try {
		const updated = await linkLocalServerInstance(props.server.id, selectedId.value)
		emit('changed')
		choosing.value = false
		if (sharesMods(updated.loader)) {
			const report = await syncProfileAndServer(updated, selectedId.value)
			addNotification({
				type: 'success',
				title: 'Verknüpft und abgeglichen',
				text: describeSync(report),
			})
			emit('changed')
		}
	} catch (e) {
		handleError(e as Error)
	} finally {
		working.value = null
	}
}

async function unlink() {
	try {
		await linkLocalServerInstance(props.server.id, null)
		emit('changed')
	} catch (e) {
		handleError(e as Error)
	}
}

async function sync() {
	if (!profile.value) return
	working.value = 'sync'
	try {
		const report = await syncProfileAndServer(props.server, profile.value.id)
		addNotification({ type: 'success', title: 'Mods abgeglichen', text: describeSync(report) })
		emit('changed')
	} catch (e) {
		handleError(e as Error)
	} finally {
		working.value = null
	}
}

async function play() {
	working.value = 'play'
	try {
		await playOnServer(props.server)
	} catch (e) {
		handleError(e as Error)
	} finally {
		working.value = null
	}
}
</script>

<template>
	<div
		class="flex flex-wrap items-center gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-4"
	>
		<template v-if="loading">
			<SpinnerIcon class="animate-spin text-secondary" />
			<span class="text-secondary">Spielprofil wird geladen…</span>
		</template>

		<!-- Linked -->
		<template v-else-if="profile">
			<Avatar
				:src="getInstanceIconUrl(profile.icon_path) ?? undefined"
				:tint-by="profile.id"
				size="48px"
				pad-transparent-corners
			/>
			<div class="min-w-0 flex-1">
				<div class="text-xs font-semibold uppercase tracking-wide text-secondary">
					Verknüpftes Spielprofil
				</div>
				<button
					type="button"
					class="truncate text-left text-lg font-bold text-contrast hover:underline"
					@click="router.push(`/instance/${encodeURIComponent(profile.id)}`)"
				>
					{{ profile.name }}
				</button>
				<div v-if="mismatch" class="text-sm text-orange">
					Das Profil nutzt {{ profile.loader }} {{ profile.game_version }}, der Server
					{{ expectedLoader }} {{ server.game_version }}.
				</div>
			</div>
			<div class="flex flex-wrap items-center gap-2">
				<Button
					v-if="sharesMods(server.loader)"
					v-tooltip="'Fehlende Mods auf beiden Seiten installieren'"
					:disabled="working !== null"
					@click="sync"
				>
					<SpinnerIcon v-if="working === 'sync'" class="animate-spin" />
					<RefreshCwIcon v-else />
					Mods abgleichen
				</Button>
				<Button type="quiet" :disabled="working !== null" @click="unlink">
					<UnlinkIcon /> Lösen
				</Button>
				<Button
					v-tooltip="server.status === 'stopped' ? 'Starte zuerst den Server' : undefined"
					type="colored"
					color="brand"
					:disabled="working !== null || server.status === 'stopped'"
					@click="play"
				>
					<SpinnerIcon v-if="working === 'play'" class="animate-spin" />
					<PlayIcon v-else />
					Spielen
				</Button>
			</div>
		</template>

		<!-- Choose an existing profile -->
		<template v-else-if="choosing">
			<div class="flex min-w-0 flex-1 flex-col gap-2">
				<div class="font-bold text-contrast">Vorhandenes Spielprofil verknüpfen</div>
				<div v-if="candidates.length === 0" class="text-sm text-secondary">
					Kein Profil mit {{ expectedLoader }} {{ server.game_version }} gefunden.
				</div>
				<Combobox
					v-else
					v-model="selectedId"
					:options="candidateOptions"
					searchable
					placeholder="Profil wählen"
				/>
			</div>
			<div class="flex items-center gap-2 self-end">
				<Button type="quiet" @click="choosing = false"><XIcon /> Abbrechen</Button>
				<Button
					type="colored"
					color="brand"
					:disabled="!selectedId || working !== null"
					@click="link"
				>
					<SpinnerIcon v-if="working === 'link'" class="animate-spin" />
					<LinkIcon v-else />
					Verknüpfen
				</Button>
			</div>
		</template>

		<!-- Not linked -->
		<template v-else>
			<div class="grid size-12 shrink-0 place-items-center rounded-xl bg-surface-4">
				<GameIcon class="size-6 text-secondary" />
			</div>
			<div class="min-w-0 flex-1">
				<div class="font-bold text-contrast">Noch kein Spielprofil verknüpft</div>
				<div class="text-sm text-secondary">
					<template v-if="sharesMods(server.loader)">
						Erstelle ein passendes Profil – gleiche Version, gleicher Loader, gleiche Mods – und
						tritt dem Server mit einem Klick bei.
					</template>
					<template v-else>
						Erstelle ein passendes Profil, um dem Server mit einem Klick beizutreten.
					</template>
				</div>
			</div>
			<div class="flex flex-wrap items-center gap-2">
				<Button :disabled="working !== null" @click="openChooser">
					<LinkIcon /> Vorhandenes verknüpfen
				</Button>
				<Button type="colored" color="brand" :disabled="working !== null" @click="createProfile">
					<SpinnerIcon v-if="working === 'create'" class="animate-spin" />
					<PlusIcon v-else />
					{{ working === 'create' ? 'Wird erstellt…' : 'Passendes Profil erstellen' }}
				</Button>
			</div>
		</template>
	</div>
</template>
