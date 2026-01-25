import * as React from "react"
import { Toast as RadixToast } from "@radix-ui/themes"

const ToastProvider = RadixToast.Provider
const ToastViewport = RadixToast.Viewport
const Toast = RadixToast.Root
const ToastTitle = RadixToast.Title
const ToastDescription = RadixToast.Description
const ToastClose = RadixToast.Close
const ToastAction = RadixToast.Action

export {
  ToastProvider,
  ToastViewport,
  Toast,
  ToastTitle,
  ToastDescription,
  ToastClose,
  ToastAction,
} 