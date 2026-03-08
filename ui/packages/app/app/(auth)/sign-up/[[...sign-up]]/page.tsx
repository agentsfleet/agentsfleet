import { SignUp } from "@clerk/nextjs";

export default function SignUpPage() {
  return (
    <SignUp
      appearance={{
        variables: {
          colorBackground: "#0f1520",
          colorInputBackground: "#161e2b",
          colorInputText: "#e8f2ff",
          colorText: "#e8f2ff",
          colorTextSecondary: "#8b97a8",
          colorPrimary: "#ff6b35",
          colorDanger: "#ff4d6a",
          borderRadius: "8px",
          fontFamily: "Geist, system-ui, sans-serif",
        },
      }}
    />
  );
}
