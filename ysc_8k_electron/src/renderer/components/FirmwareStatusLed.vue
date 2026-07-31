<template>
  <div class="fw-status-led">
    <div class="fw-led-cell fw-led-cell-updating">
      <div class="fw-led-dot fw-led-updating"></div>
      <div class="fw-led-text">
        <span class="fw-led-title">{{ t('firmwareStatus.updating') }}</span>
        <span class="fw-led-desc">{{ t('firmwareStatus.updatingDesc') }}</span>
      </div>
    </div>
    <div class="fw-led-cell fw-led-cell-done">
      <div class="fw-led-dot fw-led-done"></div>
      <div class="fw-led-text">
        <span class="fw-led-title">{{ t('firmwareStatus.done') }}</span>
        <span class="fw-led-desc">{{ t('firmwareStatus.doneDesc') }}</span>
      </div>
    </div>
    <div class="fw-led-cell fw-led-cell-working">
      <div class="fw-led-dot fw-led-working"></div>
      <div class="fw-led-text">
        <span class="fw-led-title">{{ t('firmwareStatus.working') }}</span>
        <span class="fw-led-desc">{{ t('firmwareStatus.workingDesc') }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { useI18n } from '../i18n/index.js';

// 硬件状态灯效图例（3 板块，常驻显示）：
//   黄色呼吸 = 设备处于更新模式（IAP 烧录中）
//   绿色常亮 = 更新完成
//   蓝色常亮 = 正常工作模式（APP）
// 这是给用户看的"灯效说明"，不做状态切换 —— 三种灯在任何时候都同时点亮。
const { t } = useI18n();
</script>
