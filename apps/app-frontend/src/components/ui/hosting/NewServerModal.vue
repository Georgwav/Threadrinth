<script setup lang="ts">
import { PlusIcon, SpinnerIcon, XIcon } from '@modrinth/assets'
import {
	Admonition,
	Button,
	Checkbox,
	Chips,
	Combobox,
	type ComboboxOption,
	commonMessages,
	defineMessages,
	injectNotificationManager,
	Input,
	NewModal,
	Slider,
	Toggle,
	useVIntl,
} from '@modrinth/ui'
import type { GameVersionTag } from '@modrinth/utils'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import { create_blank_server, type ServerSoftware } from '@/helpers/hosting'
import { get_game_versions } from '@/helpers/tags'

/** A server made from scratch: its software, Minecraft version and a new world. */

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const router = useRouter()

const emit = defineEmits<{
	created: [id: string]
}>()

const messages = defineMessages({
	header: { id: 'app.hosting.new.header', defaultMessage: 'New server' },
	name: { id: 'app.hosting.create.name', defaultMessage: 'Server name' },
	software: { id: 'app.hosting.new.software', defaultMessage: 'Software' },
	modLoaders: { id: 'app.hosting.new.mod-loaders', defaultMessage: 'Mods' },
	pluginLoaders: { id: 'app.hosting.new.plugin-loaders', defaultMessage: 'Plugins' },
	softwareHint: {
		id: 'app.hosting.new.software-hint',
		defaultMessage:
			'Mod loaders run mods (players may need the same mods). Paper and Purpur run plugins, which work with unmodded game clients.',
	},
	version: { id: 'app.hosting.new.version', defaultMessage: 'Minecraft version' },
	snapshots: { id: 'app.hosting.new.snapshots', defaultMessage: 'Show snapshots' },
	seed: { id: 'app.hosting.create.seed', defaultMessage: 'Seed (optional)' },
	memory: { id: 'app.hosting.create.memory', defaultMessage: 'Memory' },
	eula: { id: 'app.hosting.create.eula', defaultMessage: 'I accept the Minecraft EULA' },
	readEula: { id: 'app.hosting.create.read-eula', defaultMessage: 'Read the EULA' },
	create: { id: 'app.hosting.create.button', defaultMessage: 'Create server' },
	creating: {
		id: 'app.hosting.new.creating',
		defaultMessage:
			'Setting up the server. The first time downloads the server and Java, which can take a few minutes.',
	},
})

const SOFTWARE: { value: ServerSoftware; label: string; plugins: boolean }[] = [
	{ value: 'vanilla', label: 'Vanilla', plugins: false },
	{ value: 'fabric', label: 'Fabric', plugins: false },
	{ value: 'quilt', label: 'Quilt', plugins: false },
	{ value: 'forge', label: 'Forge', plugins: false },
	{ value: 'neoforge', label: 'NeoForge', plugins: false },
	{ value: 'paper', label: 'Paper', plugins: true },
	{ value: 'purpur', label: 'Purpur', plugins: true },
]

const modal = ref<InstanceType<typeof NewModal>>()
const name = ref<string | number | undefined>('')
const software = ref<ServerSoftware>('fabric')
const gameVersion = ref<string>()
const gameVersions = ref<GameVersionTag[]>([])
const showSnapshots = ref(false)
const seed = ref<string | number | undefined>('')
const memoryMb = ref(4096)
const eulaAccepted = ref(false)
const creating = ref(false)

const modSoftware = SOFTWARE.filter((item) => !item.plugins).map((item) => item.value)
const pluginSoftware = SOFTWARE.filter((item) => item.plugins).map((item) => item.value)
const softwareLabel = (value: ServerSoftware) =>
	SOFTWARE.find((item) => item.value === value)?.label ?? value

const versionOptions = computed<ComboboxOption<string>[]>(() =>
	gameVersions.value
		.filter((version) => showSnapshots.value || version.version_type === 'release')
		.map((version) => ({ value: version.version, label: version.version })),
)

const canCreate = computed(
	() => String(name.value ?? '').trim().length > 0 && !!gameVersion.value && !creating.value,
)

async function show() {
	name.value = 'My Server'
	software.value = 'fabric'
	seed.value = ''
	memoryMb.value = 4096
	eulaAccepted.value = false
	creating.value = false
	modal.value?.show()
	if (gameVersions.value.length === 0) {
		gameVersions.value = await get_game_versions().catch((error) => {
			handleError(error)
			return []
		})
	}
	gameVersion.value ??= gameVersions.value.find(
		(version) => version.version_type === 'release',
	)?.version
}

async function create() {
	if (!canCreate.value || !gameVersion.value) return
	creating.value = true
	try {
		const server = await create_blank_server({
			name: String(name.value ?? '').trim(),
			software: software.value,
			game_version: gameVersion.value,
			seed: String(seed.value ?? '').trim() || null,
			memory_mb: memoryMb.value,
			eula_accepted: eulaAccepted.value,
		})
		modal.value?.hide()
		emit('created', server.id)
		await router.push(`/host/${encodeURIComponent(server.id)}`)
	} catch (error) {
		handleError(error as Error)
	} finally {
		creating.value = false
	}
}

defineExpose({ show })
</script>

<template>
	<NewModal
		ref="modal"
		:header="formatMessage(messages.header)"
		scrollable
		width="40rem"
		max-width="calc(100vw - 2rem)"
	>
		<div class="flex flex-col gap-4">
			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">{{ formatMessage(messages.name) }}</span>
				<Input v-model="name" type="text" wrapper-class="w-full" />
			</label>
			<div class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">{{ formatMessage(messages.software) }}</span>
				<div class="flex flex-wrap items-center gap-2">
					<span class="w-16 text-sm text-secondary">{{ formatMessage(messages.modLoaders) }}</span>
					<Chips
						v-model="software"
						:items="modSoftware"
						:format-label="softwareLabel"
						:capitalize="false"
					/>
				</div>
				<div class="flex flex-wrap items-center gap-2">
					<span class="w-16 text-sm text-secondary">
						{{ formatMessage(messages.pluginLoaders) }}
					</span>
					<Chips
						v-model="software"
						:items="pluginSoftware"
						:format-label="softwareLabel"
						:capitalize="false"
					/>
				</div>
				<p class="m-0 text-sm text-secondary">{{ formatMessage(messages.softwareHint) }}</p>
			</div>
			<div class="flex flex-col gap-2">
				<div class="flex items-center justify-between gap-2">
					<span class="font-semibold text-contrast">{{ formatMessage(messages.version) }}</span>
					<label class="flex items-center gap-2 text-sm text-secondary">
						{{ formatMessage(messages.snapshots) }}
						<Toggle v-model="showSnapshots" />
					</label>
				</div>
				<Combobox v-model="gameVersion" :options="versionOptions" searchable sync-with-selection />
			</div>
			<Input
				v-model="seed"
				type="text"
				:placeholder="formatMessage(messages.seed)"
				wrapper-class="w-full"
			/>
			<div class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">{{ formatMessage(messages.memory) }}</span>
				<Slider v-model="memoryMb" :min="1024" :max="16384" :step="512" unit="MB" />
			</div>
			<div class="flex flex-wrap items-center gap-3">
				<Checkbox v-model="eulaAccepted" :label="formatMessage(messages.eula)" />
				<button
					class="border-none bg-transparent p-0 text-link underline"
					@click="openUrl('https://aka.ms/MinecraftEULA')"
				>
					{{ formatMessage(messages.readEula) }}
				</button>
			</div>
			<Admonition v-if="creating" type="info" :header="formatMessage(messages.creating)" />
		</div>
		<template #actions>
			<div class="flex items-center justify-end gap-2">
				<Button type="outlined" :disabled="creating" @click="modal?.hide()">
					<XIcon />
					{{ formatMessage(commonMessages.cancelButton) }}
				</Button>
				<Button type="colored" color="brand" :disabled="!canCreate" @click="create">
					<SpinnerIcon v-if="creating" class="animate-spin" />
					<PlusIcon v-else />
					{{ formatMessage(messages.create) }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>
