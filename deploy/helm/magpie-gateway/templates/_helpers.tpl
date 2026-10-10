{{- define "magpie.labels" -}}
app.kubernetes.io/name: magpie-gateway
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "magpie.selector" -}}
app.kubernetes.io/name: magpie-gateway
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/* Secret keys for the portal: MAGPIE_*. */}}
{{- define "magpie.portalSecrets" -}}
{{- range $key, $value := .Values.secrets }}
{{- if hasPrefix "MAGPIE_" $key }}
  {{ $key }}: {{ $value | quote }}
{{- end }}
{{- end }}
{{- end -}}

{{/* Secret keys for the proxy: the webhook token and provider keys. */}}
{{- define "magpie.proxySecrets" -}}
{{- range $key, $value := .Values.secrets }}
{{- if or (eq $key "MAGPIE_WEBHOOK_TOKEN") (not (hasPrefix "MAGPIE_" $key)) }}
  {{ $key }}: {{ $value | quote }}
{{- end }}
{{- end }}
{{- end -}}
