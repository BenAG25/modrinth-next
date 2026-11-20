<script setup lang="ts">
import { DownloadIcon, RefreshCwIcon, SpinnerIcon } from '@modrinth/assets'
import {
	Admonition,
	Button,
	Combobox,
	injectNotificationManager,
	NewModal,
	SegmentedControl,
} from '@modrinth/ui'
import dayjs from 'dayjs'
import { computed, nextTick, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import { list } from '@/helpers/instance'
import type { GameInstance } from '@/helpers/types'
import { injectAppEvents } from '@/providers/app-events'

import {
	decodeShareCode,
	describeApply,
	getImportRecord,
	importAsNewProfile,
	isOfflineShareCode,
	type SharePayload,
	updateProfile,
} from './share-code'
import { importOpen } from './store'

const props = defineProps<{
	/** Installs a Modrinth shared-instance invite (short code). */
	installInvite: (inviteId: string) => Promise<void> | void
}>()

const router = useRouter()
const appEvents = injectAppEvents()
const { handleError, addNotification } = injectNotificationManager()

const modal = ref<InstanceType<typeof NewModal> | null>(null)
function onHide() {
	importOpen.value = false
}
const code = ref('')
const payload = ref<SharePayload | null>(null)
const decodeError = ref('')
const instances = ref<GameInstance[]>([])
const working = ref(false)
const progress = ref('')

const TARGETS = ['Neues Profil', 'Vorhandenes aktualisieren'] as const
const target = ref<(typeof TARGETS)[number]>('Neues Profil')
const targetInstanceId = ref<string | undefined>()

const isShortCode = computed(() => {
	const text = code.value.trim()
	return !!text && !isOfflineShareCode(text)
})
const linkedInstance = computed(() => {
	if (!payload.value) return null
	const record = getImportRecord(payload.value.id)
	return record ? (instances.value.find((i) => i.id === record.instanceId) ?? null) : null
})
const instanceOptions = computed(() =>
	instances.value.map((i) => ({
		value: i.id,
		label: i.name,
		subLabel: `${i.loader} ${i.game_version}`,
	})),
)
const canImport = computed(() => {
	if (working.value) return false
	if (isShortCode.value) return true
	if (!payload.value) return false
	return target.value === 'Neues Profil' || !!targetInstanceId.value
})

/** Accepts a raw invite id or a modrinth.com/share/<id> link. */
function inviteIdFrom(text: string) {
	const trimmed = text.trim()
	const match = trimmed.match(/\/share\/([^/?#\s]+)/)
	return decodeURIComponent(match ? match[1] : trimmed)
}

let decodeTimer: ReturnType<typeof setTimeout> | undefined
watch(code, () => {
	clearTimeout(decodeTimer)
	payload.value = null
	decodeError.value = ''
	if (!isOfflineShareCode(code.value)) return
	decodeTimer = setTimeout(async () => {
		try {
			payload.value = await decodeShareCode(code.value)
			const linked = linkedInstance.value
			target.value = linked ? 'Vorhandenes aktualisieren' : 'Neues Profil'
			targetInstanceId.value = linked?.id
		} catch (e) {
			decodeError.value = (e as Error).message
		}
	}, 150)
})

watch(importOpen, async (open) => {
	if (!open) return
	code.value = ''
	progress.value = ''
	working.value = false
	instances.value = await list().catch(() => [])
	await nextTick()
	modal.value?.show()
})

async function runImport() {
	if (!canImport.value) return
	if (isShortCode.value) {
		modal.value?.hide()
		await props.installInvite(inviteIdFrom(code.value))
		return
	}
	if (!payload.value) return

	working.value = true
	const onProgress = (message: string) => (progress.value = message)
	try {
		if (target.value === 'Neues Profil') {
			const { instanceId, report } = await importAsNewProfile(payload.value, appEvents, onProgress)
			addNotification({
				type: 'success',
				title: `„${payload.value.name}“ importiert`,
				text: describeApply(report),
			})
			modal.value?.hide()
			await router.push(`/instance/${encodeURIComponent(instanceId)}`)
		} else if (targetInstanceId.value) {
			const report = await updateProfile(
				targetInstanceId.value,
				payload.value,
				appEvents,
				onProgress,
			)
			addNotification({
				type: 'success',
				title: 'Profil aktualisiert',
				text: describeApply(report),
			})
			const id = targetInstanceId.value
			modal.value?.hide()
			await router.push(`/instance/${encodeURIComponent(id)}`)
		}
	} catch (e) {
		handleError(e as Error)
	} finally {
		working.value = false
	}
}
</script>

<template>
	<NewModal
		ref="modal"
		header="Profil mit Code importieren"
		max-width="600px"
		width="600px"
		:on-hide="onHide"
	>
		<div class="flex flex-col gap-4">
			<p class="m-0 text-secondary">
				Füge den Code ein, den du bekommen hast – einen langen Modrinth-Next-Code oder einen kurzen
				Modrinth-Code.
			</p>
			<textarea
				v-model="code"
				rows="4"
				placeholder="Code hier einfügen"
				:disabled="working"
				class="w-full resize-none break-all rounded-xl border border-solid border-surface-4 bg-surface-2 p-3 font-mono text-sm text-primary"
			/>

			<Admonition v-if="decodeError" type="critical">{{ decodeError }}</Admonition>

			<Admonition v-else-if="isShortCode" type="info">
				Kurzer Modrinth-Code: Du wirst ggf. gebeten, dich mit deinem Modrinth-Account anzumelden.
			</Admonition>

			<template v-else-if="payload">
				<div class="rounded-2xl border border-solid border-surface-4 bg-surface-2 p-4">
					<div class="text-lg font-bold text-contrast">{{ payload.name }}</div>
					<div class="text-sm text-secondary">
						{{ payload.loader }} {{ payload.game }} · {{ payload.files.length }} Inhalte · Stand
						{{ dayjs.unix(payload.rev).format('DD.MM.YYYY HH:mm') }}
					</div>
				</div>

				<SegmentedControl
					v-model="target"
					:items="[...TARGETS]"
					aria-label="Ziel"
					full-width
					equal-width
					:disabled="working"
				/>
				<div v-if="target === 'Vorhandenes aktualisieren'" class="flex flex-col gap-2">
					<Combobox
						v-model="targetInstanceId"
						:options="instanceOptions"
						searchable
						placeholder="Profil wählen"
						:disabled="working"
						:max-height="300"
					/>
					<div class="text-sm text-secondary">
						<template v-if="linkedInstance && targetInstanceId === linkedInstance.id">
							Dieses Profil wurde schon mit einem früheren Code dieses Profils erstellt. Neue Mods
							werden hinzugefügt, geänderte aktualisiert und weggefallene entfernt.
						</template>
						<template v-else>
							Fehlende Mods werden hinzugefügt und andere Versionen ersetzt. Deine eigenen Mods
							bleiben erhalten.
						</template>
					</div>
				</div>
			</template>

			<Admonition v-if="working" type="info" :header="progress || 'Wird importiert…'">
				Das kann je nach Anzahl der Mods einen Moment dauern.
			</Admonition>

			<div class="flex justify-end gap-2">
				<Button type="quiet" :disabled="working" @click="modal?.hide()">Abbrechen</Button>
				<Button type="colored" color="brand" :disabled="!canImport" @click="runImport">
					<SpinnerIcon v-if="working" class="animate-spin" />
					<RefreshCwIcon v-else-if="payload && target === 'Vorhandenes aktualisieren'" />
					<DownloadIcon v-else />
					{{ payload && target === 'Vorhandenes aktualisieren' ? 'Aktualisieren' : 'Importieren' }}
				</Button>
			</div>
		</div>
	</NewModal>
</template>
