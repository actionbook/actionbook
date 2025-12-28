export const metadata = {
  title: 'Actionbook API Service',
  description: 'Mock API service for MCP testing',
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}
