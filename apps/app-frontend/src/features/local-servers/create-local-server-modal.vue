<script setup lang="ts">
import { PlusIcon, SpinnerIcon, XIcon } from '@modrinth/assets'
import {
	Admonition,
	Button,
	Checkbox,
	Combobox,
	injectNotificationManager,
	NewModal,
	Slider,
} from '@modrinth/ui'
import { computed, onBeforeUnmount, ref, watch } from 'vue'

import {
	createLocalServer,
	getLocalServerGameVersions,
	getLocalServerLoaderVersions,
	onLocalServerProgress,
} from '@/platform/app-lib/local-servers/commands'
import type { LocalServer, LocalServerLoader } from '@/platform/app-lib/local-servers/types'
import { openExternalUrl } from '@/platform/external-links'

import { LOADERS } from './loaders'

const emit = defineEmits<{ created: [server: LocalServer] }>()
const { handleError } = injectNotificationManager()

const modal = ref<InstanceType<typeof NewModal> | null>(null)

const name = ref('')
const loader = ref<LocalServerLoader>('fabric')
const gameVersion = ref<string | undefined>()
const loaderVersion = ref<string>('')
const memoryGb = ref(4)
const port = ref(25565)
const acceptEula = ref(false)
const showSnapshots = ref(false)

const gameVersions = ref<string[]>([])
const loaderVersions = ref<string[]>([])
const loadingVersions = ref(false)
const creating = ref(false)
const progress = ref('')

let unlistenProgress: (() => void) | null = null

const usesLoaderVersion = computed(() => ['fabric', 'forge', 'neoforge'].includes(loader.value))

const gameVersionOptions = computed(() => gameVersions.value.map((v) => ({ value: v, label: v })))
const loaderVersionOptions = computed(() => [
	{ value: '', label: 'Neueste (empfohlen)' },
	...loaderVersions.value.map((v) => ({ value: v, label: v })),
])

const canCreate = computed(
	() => !!name.value.trim() && !!gameVersion.value && acceptEula.value && !creating.value,
)

async function loadGameVersions() {
	loadingVersions.value = true
	try {
		const versions = await getLocalServerGameVersions(loader.value, showSnapshots.value)
		gameVersions.value = versions
		if (!gameVersion.value || !versions.includes(gameVersion.value)) {
			gameVersion.value = versions[0]
		}
	} catch (e) {
		gameVersions.value = []
		handleError(e as Error)
	} finally {
		loadingVersions.value = false
	}
}

async function loadLoaderVersions() {
	loaderVersion.value = ''
	loaderVersions.value = []
	if (!usesLoaderVersion.value || !gameVersion.value) return
	try {
		loaderVersions.value = await getLocalServerLoaderVersions(loader.value, gameVersion.value)
	} catch {
		loaderVersions.value = []
	}
}

watch([loader, showSnapshots], loadGameVersions)
watch([loader, gameVersion], loadLoaderVersions)

async function show() {
	name.value = ''
	acceptEula.value = false
	progress.value = ''
	creating.value = false
	modal.value?.show()
	if (gameVersions.value.length === 0) await loadGameVersions()
}

async function create() {
	if (!canCreate.value || !gameVersion.value) return
	creating.value = true
	progress.value = 'Server wird erstellt…'
	unlistenProgress?.()
	unlistenProgress = await onLocalServerProgress((event) => {
		progress.value = event.message
	})
	try {
		const server = await createLocalServer({
			name: name.value.trim(),
			loader: loader.value,
			game_version: gameVersion.value,
			loader_version: loaderVersion.value || null,
			memory_mb: Math.round(memoryGb.value * 1024),
			port: port.value,
			accept_eula: acceptEula.value,
		})
		modal.value?.hide()
		emit('created', server)
	} catch (e) {
		handleError(e as Error)
	} finally {
		creating.value = false
		unlistenProgress?.()
		unlistenProgress = null
	}
}

onBeforeUnmount(() => unlistenProgress?.())

defineExpose({ show })
</script>

<template>
	<NewModal ref="modal" header="Lokalen Server erstellen" max-width="560px" width="560px">
		<div class="flex flex-col gap-5">
			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Name</span>
				<input
					v-model="name"
					type="text"
					maxlength="64"
					placeholder="Mein Server"
					:disabled="creating"
					autofocus
				/>
			</label>

			<div class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Server-Software</span>
				<div class="grid grid-cols-5 gap-2">
					<button
						v-for="l in LOADERS"
						:key="l.id"
						type="button"
						class="flex flex-col items-center gap-1 rounded-xl border-2 border-solid px-2 py-3 text-sm transition-colors"
						:class="
							loader === l.id
								? 'border-brand bg-brand-highlight text-contrast'
								: 'border-transparent bg-button-bg text-primary hover:brightness-110'
						"
						:disabled="creating"
						@click="loader = l.id"
					>
						<span class="font-bold">{{ l.label }}</span>
					</button>
				</div>
				<span class="text-sm text-secondary">
					{{ LOADERS.find((l) => l.id === loader)?.description }}
				</span>
			</div>

			<div class="grid grid-cols-2 gap-4">
				<div class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Minecraft-Version</span>
					<Combobox
						v-model="gameVersion"
						:options="gameVersionOptions"
						searchable
						:disabled="creating || loadingVersions"
						:placeholder="loadingVersions ? 'Lädt…' : 'Version wählen'"
						:max-height="300"
					/>
					<Checkbox v-model="showSnapshots" label="Snapshots anzeigen" :disabled="creating" />
				</div>
				<div v-if="usesLoaderVersion" class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Loader-Version</span>
					<Combobox
						v-model="loaderVersion"
						:options="loaderVersionOptions"
						searchable
						:disabled="creating"
						:max-height="300"
					/>
				</div>
			</div>

			<div class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Arbeitsspeicher: {{ memoryGb }} GB</span>
				<Slider v-model="memoryGb" :min="1" :max="16" :step="0.5" unit=" GB" :disabled="creating" />
			</div>

			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Port</span>
				<input v-model.number="port" type="number" min="1024" max="65535" :disabled="creating" />
			</label>

			<div class="flex flex-col gap-1">
				<Checkbox v-model="acceptEula" :disabled="creating">
					Ich akzeptiere die Minecraft-EULA
				</Checkbox>
				<button
					type="button"
					class="w-fit text-left text-sm text-link hover:underline"
					@click="openExternalUrl('https://aka.ms/MinecraftEULA')"
				>
					EULA lesen
				</button>
			</div>

			<Admonition v-if="creating" type="info" :header="progress">
				Beim ersten Mal werden Server-Dateien und ggf. Java heruntergeladen.
			</Admonition>

			<div class="flex justify-end gap-2">
				<Button type="outlined" :disabled="creating" @click="modal?.hide()">
					<XIcon /> Abbrechen
				</Button>
				<Button type="colored" color="brand" :disabled="!canCreate" @click="create">
					<SpinnerIcon v-if="creating" class="animate-spin" />
					<PlusIcon v-else />
					Server erstellen
				</Button>
			</div>
		</div>
	</NewModal>
</template>
