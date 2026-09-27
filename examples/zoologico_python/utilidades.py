from animal import Animal


def describir(animal: Animal) -> str:
    return f"{animal.nombre} es un {type(animal).__name__}"


def saludar(nombre: str) -> None:
    print(f"Bienvenido al zoologico, {nombre}")
